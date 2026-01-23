// Copyright (c) 2023 Alibaba Cloud
//
// SPDX-License-Identifier: Apache-2.0
//

use anyhow::*;
use base64::Engine;
use jwt_simple::prelude::{Claims, Duration, Ed25519KeyPair, EdDSAKeyPairLike};
use kbs_protocol::{
    evidence_provider::NativeEvidenceProvider, KbsClientBuilder, KbsClientCapabilities, ResourceUri,
};
use log::debug;
use reqwest::Url;

const KBS_URL_PATH_PREFIX: &str = "kbs/v0/resource";

/// Get the key from KBS using the KBS protocol with attestation.
/// This function performs attestation and retrieves the key securely.
pub(crate) async fn get_kek(kbs_addr: &Url, kid: &str) -> Result<Vec<u8>> {
    let kid = kid.strip_prefix('/').unwrap_or(kid);

    // Construct the resource URI in the format: kbs:///<repository>/<type>/<tag>
    let resource_uri_str = format!("kbs:///{}", kid);
    debug!(
        "Fetching KEK from KBS with resource URI: {}",
        resource_uri_str
    );

    let resource_uri: ResourceUri = resource_uri_str
        .as_str()
        .try_into()
        .map_err(|e| anyhow!("Failed to parse resource URI: {}", e))?;

    // Create or reuse KBS client with attestation
    let evidence_provider = NativeEvidenceProvider::new()
        .context("Failed to create evidence provider for attestation")?;

    let mut kbs_client =
        KbsClientBuilder::with_evidence_provider(Box::new(evidence_provider), kbs_addr.as_str())
            .build()
            .context("Failed to build KBS client")?;

    debug!("Performing attestation and fetching KEK from KBS");
    let mut key = kbs_client
        .get_resource(resource_uri)
        .await
        .context("Failed to get resource from KBS (attestation may have failed)")?;

    debug!("Retrieved KEK from KBS ({} bytes)", key.len());

    // If the key is not 32 bytes (256-bit AES key), try base64 decoding
    // Some KBS implementations return base64-encoded keys or text with newlines
    if key.len() != 32 {
        debug!(
            "KEK is not 32 bytes (got {} bytes), attempting base64 decode",
            key.len()
        );

        // First, try to interpret as UTF-8 string and trim whitespace
        // This handles cases where KBS returns "base64string\n"
        let key_str = String::from_utf8_lossy(&key);
        let trimmed = key_str.trim();

        debug!(
            "Original length: {}, Trimmed length: {}",
            key_str.len(),
            trimmed.len()
        );

        let engine = base64::engine::general_purpose::STANDARD;
        let decoded = engine.decode(trimmed.as_bytes()).context(format!(
            "KBS returned key with invalid length: {} bytes (expected 32 bytes). \
             Attempted base64 decode failed. Key data (trimmed): '{}'",
            key.len(),
            trimmed
        ))?;

        if decoded.len() == 32 {
            debug!("Successfully decoded base64 KEK to 32 bytes");
            key = decoded;
        } else {
            bail!(
                "KBS returned key with invalid length: {} bytes (expected 32 bytes). \
                 Base64 decode resulted in {} bytes.",
                key.len(),
                decoded.len()
            );
        }
    }

    debug!("Successfully retrieved and validated KEK from KBS (32 bytes)");

    Ok(key)
}

/// Register the given key with kid into the kbs. This request will be authorized with a
/// JWT token, which will be signed by the private_key.
pub(crate) async fn register_kek(
    private_key: &Ed25519KeyPair,
    kbs_addr: &Url,
    key: Vec<u8>,
    kid: &str,
) -> Result<()> {
    let kid = kid.strip_prefix('/').unwrap_or(kid);
    let claims = Claims::create(Duration::from_hours(2));
    let token = private_key.sign(claims)?;
    debug!("sign claims.");

    let client = reqwest::Client::new();
    let mut resource_url = kbs_addr.clone();

    let path = format!("{KBS_URL_PATH_PREFIX}/{kid}");

    resource_url.set_path(&path);

    debug!("register KEK into {resource_url}");
    let _ = client
        .post(resource_url)
        .header("Content-Type", "application/octet-stream")
        .bearer_auth(token)
        .body(key)
        .send()
        .await?;

    Ok(())
}
