use std::{process::Command, fmt};
use anyhow::{bail, Result};
use serde::{Serialize, Deserialize};
use tracing_opentelemetry_instrumentation_sdk::find_current_trace_id;

use crate::errors::{ProvisioningError, ProvisioningErrorCodes};

/**
 * Open SSL Commands Reference
 * 
 * [Default]
 * ECDSA:
 * openssl ecparam -name secp521r1 -genkey -noout -out key.pem
 * openssl req -new -sha256 -key key.pem -out req.pem
 * 
 * RSA:
 * openssl genrsa -out key.pem 2048
 * openssl req -new -sha256 -key key.pem -out req.pem
 * 
 * [TrustM]
 * TBD
 * 
 */

// Certificate Attributes
 #[derive(Serialize, Deserialize, Debug)]
pub struct CertificateAttributes {
    pub country: Option<String>,
    pub state: Option<String>,
    pub organization: Option<String>,
    pub common_name: String,
}

// Key algorithm enum
#[derive(Serialize, Deserialize, Debug)]
pub enum PrivateKeyAlgorithm {
    ECDSA,
}

// Key size enum
#[derive(Serialize, Deserialize, Debug)]
pub enum PrivateKeySize {
    #[serde(rename = "EC_P256")]
    EcP256,
    #[serde(rename = "EC_P384")]
    EcP384,
    #[serde(rename = "EC_P521")]
    EcP521,
}

impl fmt::Display for PrivateKeySize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PrivateKeySize::EcP256 => write!(f, "EC_P256"),
            PrivateKeySize::EcP384 => write!(f, "EC_P384"),
            PrivateKeySize::EcP521 => write!(f, "EC_P521"),
        }
    }
}

pub fn generate_ec_private_key(file_path: &str, key_size: PrivateKeySize) -> Result<bool> {
    let trace_id = find_current_trace_id();
    tracing::trace!(trace_id, task = "generate_ec_private_key", "init",);

    let elliptic_curve = match key_size {
        PrivateKeySize::EcP256 => String::from("secp256r1"),
        PrivateKeySize::EcP384 => String::from("secp384r1"),
        PrivateKeySize::EcP521 => String::from("secp521r1"),
        // k => bail!(ProvisioningError::new(
        //     ProvisioningErrorCodes::CryptoGeneratePrivateKeyError,
        //     format!("key size not supported for elliptical curve key - {}", k),
        //     true
        // ))
    };
    
    // Command: openssl ecparam -name secp521r1 -genkey -noout -out key.pem
    let output_result = Command::new("openssl")
        .arg("ecparam")
        .arg("-name")
        .arg(elliptic_curve)
        .arg("-genkey")
        .arg("-noout")
        .arg("-out")
        .arg(file_path)
        .output();

    let output = match output_result {
        Ok(v) => v,
        Err(e) => bail!(ProvisioningError::new(
            ProvisioningErrorCodes::CryptoGeneratePrivateKeyError,
            format!("openssl private key generate command failed - {}", e),
            true
        ))
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        bail!(ProvisioningError::new(
            ProvisioningErrorCodes::CryptoGeneratePrivateKeyError,
            format!("openssl error in generating private key, stderr - {}", stderr),
            true
        ))
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    tracing::info!(
        trace_id,
        task = "generate_ec_private_key",
        result = "success",
        "openssl ec private key generated successfully",
    );
    tracing::trace!(
        trace_id,
        task = "generate_ec_private_key",
        "openssl ec private key generate command stdout - {}",
        stdout,
    );

    // TODO: Update permissions of keypath to 400
    Ok(true)
}


pub fn generate_csr(file_path: &str, private_key_path: &str, common_name: &str) -> Result<bool> {
    let trace_id = find_current_trace_id();
    tracing::trace!(trace_id, task = "generate_csr", "init",);

    let subject = format!("/C=/ST=/L=/O=/OU=/CN={}", common_name);
    // Command: openssl req -new -sha256 -key key.pem -subj "/C=/ST=/L=/O=/OU=/CN=" -out req.pem
    let output_result = Command::new("openssl")
        .arg("req")
        .arg("-new")
        .arg("-sha256")
        .arg("-key")
        .arg(private_key_path)
        .arg("-subj")
        .arg(subject)
        .arg("-out")
        .arg(file_path)
        .output();

    let output = match output_result {
        Ok(v) => v,
        Err(e) => bail!(ProvisioningError::new(
            ProvisioningErrorCodes::CryptoGenerateCSRError,
            format!("openssl csr generate command failed - {}", e),
            true
        ))
    };

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        tracing::info!(
            trace_id,
            task = "generate_csr",
            result = "success",
            "openssl csr generated successfully",
        );
        tracing::trace!(
            trace_id,
            task = "generate_csr",
            "openssl csr generate command stdout - {}",
            stdout,
        );
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        bail!(ProvisioningError::new(
            ProvisioningErrorCodes::CryptoGenerateCSRError,
            format!("openssl error in generating csr, stderr - {}", stderr),
            true
        ))
    }
}
