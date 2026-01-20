use anyhow::{Context, Result};
use attestation_agent::{AttestationAPIs, AttestationAgent};
use clap::Parser;

use hex;
use image_rs::layer_store::LayerStore;
use image_rs::meta_store::MetaStore;
use image_rs::pull::PullClient;
use kbs_protocol::client::KbsClient;
use kbs_protocol::evidence_provider::NativeEvidenceProvider;
use kbs_protocol::KbsClientBuilder;
use kbs_protocol::KbsClientCapabilities;
use oci_client::client::ClientConfig;
use oci_client::secrets::RegistryAuth;
use oci_client::Reference;
use oci_spec::image::ImageConfiguration;
use resource_uri::ResourceUri;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

type KbsClientType = KbsClient<Box<dyn kbs_protocol::evidence_provider::EvidenceProvider>>;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// OCI image reference (e.g., docker.io/user/wasm:tag)
    #[arg(short, long)]
    image_reference: String,

    /// Working directory for WASM execution
    #[arg(short, long, default_value = "/tmp/tee-wasm-runner")]
    work_dir: PathBuf,

    /// Directory to store pulled image layers
    #[arg(short, long, default_value = "/tmp/tee-wasm-runner/layers")]
    layer_store_path: PathBuf,

    /// Key Broker Service URI (required for encrypted images)
    #[arg(short, long)]
    kbs_uri: Option<String>,

    /// Key Broker Client name
    #[arg(short = 'n', long, default_value = "sample")]
    kbc_name: String,

    /// Attestation Agent configuration file
    #[arg(short, long)]
    aa_config: Option<String>,

    /// KBS resource path for decryption key (e.g., default/key/wasm-addition)
    #[arg(long, default_value = "default/key/encryption-key")]
    kbs_resource_path: String,

    /// WASM runtime to use (default: wasmtime)
    #[arg(short = 'r', long = "runtime", default_value = "wasmtime")]
    wasm_runtime: String,

    /// Function to invoke in the WASM module (for wasmtime --invoke)
    #[arg(long)]
    invoke: Option<String>,

    /// Arguments to pass to the WASM module
    #[arg(long)]
    wasm_args: Vec<String>,
}

struct TeeWasmRunner {
    args: Args,
    attestation_agent: AttestationAgent,
}

impl TeeWasmRunner {
    /// Create a new TEE WASM Runner instance
    async fn new(args: Args) -> Result<Self> {
        let mut attestation_agent = AttestationAgent::new(args.aa_config.as_deref())
            .context("Failed to create attestation agent")?;
        attestation_agent.init().await?;

        Ok(Self {
            args,
            attestation_agent,
        })
    }

    /// Setup KBS client for encrypted image decryption
    async fn setup_kbs_client(&self) -> Result<KbsClientType> {
        let kbs_uri = self
            .args
            .kbs_uri
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("KBS URI is required for encrypted images"))?;

        log::info!("Setting up KBS client with URI: {}", kbs_uri);

        let evidence_provider = Box::new(NativeEvidenceProvider::new()?);

        let client =
            KbsClientBuilder::with_evidence_provider(evidence_provider, kbs_uri).build()?;

        Ok(client)
    }

    /// Get decryption key from KBS
    async fn get_decryption_key(&self, _client: &mut KbsClientType) -> Result<Vec<u8>> {
        let kbs_uri = self
            .args
            .kbs_uri
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("KBS URI is required for encrypted images"))?;

        log::info!("Setting up KBS client with URI: {}", kbs_uri);

        let evidence_provider = Box::new(NativeEvidenceProvider::new()?);

        let mut client =
            KbsClientBuilder::with_evidence_provider(evidence_provider, kbs_uri).build()?;

        // Get resource from KBS (may be base64 encoded)
        let resource_path = &self.args.kbs_resource_path;
        log::info!("Using KBS resource path: {}", resource_path);

        // Extract the host from kbs_uri (e.g., "http://10.0.2.2:8082" -> "10.0.2.2:8082")
        let kbs_host = kbs_uri
            .trim_start_matches("http://")
            .trim_start_matches("https://");

        // Construct full KBS resource URI: kbs://<host>/<repo>/<type>/<tag>
        let full_resource_uri = format!("kbs://{}/{}", kbs_host, resource_path);
        log::info!("Constructed full KBS resource URI: {}", full_resource_uri);

        let resource_uri = ResourceUri::try_from(full_resource_uri.as_str())
            .map_err(|e| anyhow::anyhow!("Failed to create resource URI: {}", e))?;

        log::info!("Fetching resource from KBS: {:?}", resource_uri);

        let key = client
            .get_resource(resource_uri)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get decryption key from KBS: {}", e))?;

        log::info!("Received key from KBS: {} bytes", key.len());
        log::info!("Key from KBS (hex): {}", hex::encode(&key));
        log::info!(
            "Key from KBS (base64): {}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &key)
        );
        // Also try to interpret as UTF-8 string (in case it's base64 encoded in KBS)
        if let Ok(key_str) = std::str::from_utf8(&key) {
            log::info!("Key from KBS (as string): {}", key_str);
        }

        // Validate key before returning
        if key.is_empty() {
            return Err(anyhow::anyhow!(
                "Invalid decryption key from KBS: Empty key"
            ));
        }

        log::info!("Decryption key from KBS: {} bytes", key.len());

        Ok(key)
    }

    /// Pull and decrypt WASM image from registry
    async fn pull_and_decrypt_wasm(&self) -> Result<PathBuf> {
        let image_ref = Reference::try_from(self.args.image_reference.clone())
            .context("Failed to parse image reference")?;

        log::info!("Pulling image: {}", image_ref);

        let layer_store = LayerStore::new(self.args.layer_store_path.clone())
            .context("Failed to create layer store")?;

        let client_config = ClientConfig::default();

        let mut pull_client = PullClient::new(
            image_ref.clone(),
            layer_store,
            &RegistryAuth::Anonymous,
            4,
            client_config,
        )?;

        // Pull manifest and config
        let (manifest, _digest, config) = pull_client
            .pull_manifest()
            .await
            .context("Failed to pull manifest")?;

        log::info!("Successfully pulled manifest for image: {}", image_ref);

        // Check if this is a WASM image or standard OCI image
        let is_wasm_image = manifest.config.media_type.contains("wasm")
            || manifest
                .layers
                .iter()
                .any(|l| l.media_type.contains("wasm"));

        // Check if image is encrypted
        let is_encrypted = manifest
            .layers
            .iter()
            .any(|l| l.media_type.contains("encrypted"));

        log::info!("Image type: {}", if is_wasm_image { "WASM" } else { "OCI" });
        log::info!("Image encrypted: {}", is_encrypted);

        // For WASM images, download blob directly instead of using layer decompression
        // BUT: If encrypted, use standard OCI path to handle decryption
        if is_wasm_image && !is_encrypted {
            log::info!("Pulling WASM blob directly");

            let wasm_layer = manifest
                .layers
                .first()
                .ok_or_else(|| anyhow::anyhow!("No layers found in WASM image"))?;

            log::info!(
                "WASM layer: {} ({})",
                wasm_layer.digest,
                wasm_layer.media_type
            );

            // Pull the blob stream
            let blob_stream = pull_client
                .client
                .pull_blob_stream(&image_ref, wasm_layer)
                .await
                .context("Failed to pull WASM blob")?;

            // Write blob to file
            let wasm_filename = format!("{}.wasm", wasm_layer.digest.replace("sha256:", ""));
            let wasm_path = self.args.layer_store_path.join(wasm_filename);

            log::info!("Writing WASM to: {:?}", wasm_path);

            let mut file = tokio::fs::File::create(&wasm_path)
                .await
                .context("Failed to create WASM file")?;

            use futures_util::StreamExt;
            let mut stream = blob_stream.stream;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("Failed to read blob chunk")?;
                file.write_all(&chunk)
                    .await
                    .context("Failed to write WASM chunk")?;
            }

            file.sync_all().await.context("Failed to sync WASM file")?;

            log::info!("Successfully pulled WASM to: {:?}", wasm_path);

            Ok(wasm_path)
        } else {
            // Standard OCI image processing OR encrypted WASM (needs decryption)
            // Note: WASM images may have invalid/minimal config, so we try to parse it
            // but OCI image processing will handle the actual layer operations
            log::info!("Processing with OCI layer handler (for decryption if encrypted)");

            // WASM images often have empty/invalid configs, handle gracefully
            // Use default config if parsing fails (common for wasm-to-oci created images)
            let image_config =
                ImageConfiguration::from_reader(config.as_bytes()).unwrap_or_else(|e| {
                    log::warn!(
                        "Failed to parse image config (may be minimal WASM config): {}",
                        e
                    );
                    log::info!("Using default OCI configuration for WASM image");
                    ImageConfiguration::default()
                });

            let diff_ids = image_config.rootfs().diff_ids();

            // For WASM images with empty configs, generate placeholder diff_ids
            let diff_ids_vec: Vec<String> = if diff_ids.is_empty() && !manifest.layers.is_empty() {
                log::info!(
                    "Image config has no diff_ids, generating placeholders for {} layers",
                    manifest.layers.len()
                );
                manifest
                    .layers
                    .iter()
                    .map(|layer| {
                        // For encrypted images, we can't know the diff_id until after decryption
                        // Use empty string to skip digest validation
                        if is_encrypted {
                            log::info!("Using empty diff_id for encrypted layer (digest validation will be skipped)");
                            String::new()
                        } else {
                            // For non-encrypted images, use layer digest as placeholder diff_id
                            layer.digest.clone()
                        }
                    })
                    .collect()
            } else {
                diff_ids.to_vec()
            };

            // For encrypted images, the gRPC keyprovider (attestation-agent) handles
            // key fetching from KBS automatically based on the layer annotations.
            // Make sure:
            // 1. OCICRYPT_KEYPROVIDER_CONFIG env var points to keyprovider config
            // 2. attestation-agent is running with gRPC keyprovider socket
            let decrypt_config: Option<String> = if is_encrypted {
                log::info!(
                    "Encrypted image detected - keyprovider will handle decryption via gRPC"
                );
                log::info!("Ensure OCICRYPT_KEYPROVIDER_CONFIG is set, e.g.:");
                log::info!("  export OCICRYPT_KEYPROVIDER_CONFIG=/etc/ocicrypt_keyprovider.conf");
                log::info!("And attestation-agent is running with --keyprovider_sock");
                // The keyprovider protocol uses the annotation (e.g., kbs:///default/key/name)
                // to fetch key from KBS via attestation-agent
                Some("provider:attestation-agent".to_string())
            } else {
                log::info!("No encrypted layers detected");
                None
            };

            // Pull and decrypt layers
            let layer_metas = pull_client
                .async_pull_layers(
                    manifest.layers.clone(),
                    &diff_ids_vec,
                    &decrypt_config.as_deref(),
                    Arc::new(RwLock::new(MetaStore::default())),
                )
                .await
                .context("Failed to pull and decrypt layers")?;

            let layer_store_path = layer_metas
                .first()
                .map(|m| PathBuf::from(&m.store_path))
                .ok_or_else(|| anyhow::anyhow!("No layers found in image"))?;

            log::info!("Layer store path: {:?}", layer_store_path);

            // Find the WASM file in the extracted layer directory
            // WASM layers are written as module.wasm inside the store path
            let wasm_path = layer_store_path.join("module.wasm");
            log::info!("Checking for WASM at: {:?}", wasm_path);

            if wasm_path.exists() && wasm_path.is_file() {
                log::info!("Found WASM module at: {:?}", wasm_path);
                return Ok(wasm_path);
            }

            // Fallback: search for any .wasm file in the directory
            log::info!("module.wasm not found, searching directory...");

            // List directory contents for debugging
            if let Ok(entries) = std::fs::read_dir(&layer_store_path) {
                let files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                log::info!("Directory contents ({} entries):", files.len());
                for entry in &files {
                    log::info!(
                        "  - {:?} (is_file: {})",
                        entry.path(),
                        entry.path().is_file()
                    );
                }

                // Find first .wasm file
                for entry in files {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension() {
                            if ext == "wasm" {
                                log::info!("Found WASM file: {:?}", path);
                                return Ok(path);
                            }
                        }
                    }
                }
            }

            Err(anyhow::anyhow!(
                "No WASM file found in layer store: {:?}",
                layer_store_path
            ))
        }
    }

    /// Run WASM module using wasmtime CLI
    fn run_wasm(&self, wasm_path: &PathBuf) -> Result<()> {
        log::info!("Running WASM with {} runtime", self.args.wasm_runtime);
        log::info!("WASM path: {:?}", wasm_path);
        log::info!("WASI dir: {:?}", self.args.work_dir);

        let mut cmd = Command::new(&self.args.wasm_runtime);

        // Add --invoke flag if specified
        if let Some(ref func_name) = self.args.invoke {
            log::info!("Invoking function: {}", func_name);
            cmd.arg("--invoke").arg(func_name);
        }

        // Setup wasmtime with WASI directory access
        cmd.arg("--dir").arg(&self.args.work_dir).arg(wasm_path);

        // Add user-provided arguments
        for arg in &self.args.wasm_args {
            cmd.arg(arg);
        }

        log::info!("Executing command: {:?}", cmd);

        let output = cmd.output().context("Failed to execute WASM runtime")?;

        // Print stdout and stderr
        if !output.stdout.is_empty() {
            log::info!("WASM stdout:\n{}", String::from_utf8_lossy(&output.stdout));
        }

        if !output.stderr.is_empty() {
            log::warn!("WASM stderr:\n{}", String::from_utf8_lossy(&output.stderr));
        }

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "WASM execution failed with exit code: {:?}",
                output.status.code()
            ));
        }

        log::info!("WASM execution completed successfully");

        Ok(())
    }

    /// Main execution flow
    async fn run(&self) -> Result<()> {
        log::info!("Starting TEE WASM Runner...");
        log::info!("Image: {}", self.args.image_reference);
        log::info!("TEE Platform: {:?}", self.attestation_agent.get_tee_type());

        // Get TEE evidence
        let evidence = self
            .attestation_agent
            .get_evidence(b"wasm-runner")
            .await
            .context("Failed to get TEE evidence")?;
        log::info!("TEE evidence obtained: {} bytes", evidence.len());

        // Pull and decrypt WASM
        let wasm_path = self.pull_and_decrypt_wasm().await?;

        // Run WASM
        self.run_wasm(&wasm_path)?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();

    // Create necessary directories
    std::fs::create_dir_all(&args.work_dir).context("Failed to create work directory")?;
    std::fs::create_dir_all(&args.layer_store_path)
        .context("Failed to create layer store directory")?;

    // Run the TEE WASM Runner
    let runner = TeeWasmRunner::new(args).await?;

    if let Err(e) = runner.run().await {
        log::error!("Error running TEE WASM runner: {:?}", e);
        std::process::exit(1);
    }

    Ok(())
}
