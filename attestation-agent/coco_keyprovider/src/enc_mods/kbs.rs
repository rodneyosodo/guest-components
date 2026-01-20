// Copyright (c) 2023 Alibaba Cloud
//
// SPDX-License-Identifier: Apache-2.0
//

use anyhow::*;
use jwt_simple::prelude::{Claims, Duration, Ed25519KeyPair, EdDSAKeyPairLike};
use kbs_protocol::{
    evidence_provider::NativeEvidenceProvider, KbsClientBuilder, KbsClientCapabilities, ResourceUri,
};
use log::debug;
use reqwest::Url;
use std::sync::{Arc, Mutex};

const KBS_URL_PATH_PREFIX: &str = "kbs/v0/resource";

/// Global KBS client cache to reuse attestation sessions
static KBS_CLIENT_CACHE: once_cell::sync::Lazy<
    Arc<
        Mutex<
            Option<
                kbs_protocol::client::KbsClient<
                    Box<dyn kbs_protocol::evidence_provider::EvidenceProvider>,
                >,
            >,
        >,
    >,
> = once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

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
    let key = kbs_client
        .get_resource(resource_uri)
        .await
        .context("Failed to get resource from KBS (attestation may have failed)")?;

    debug!("Successfully retrieved KEK from KBS ({} bytes)", key.len());

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
