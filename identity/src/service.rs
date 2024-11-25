use agent_settings::constants;
use anyhow::{bail, Result};
use crypto::MachineCert;
use fs::construct_dir_path;
use tracing::{error, info, trace};

#[derive(Debug)]
pub struct MachineDetails {
    pub machine_id: String,
    pub certificate_serial_number: String,
    pub certificate_fingerprint: String,
}
const PACKAGE_NAME: &str = env!("CARGO_CRATE_NAME");
pub fn get_provision_status(data_dir: &str) -> Result<bool> {
    let fn_name = "get_provision_status";
    trace!(
        func = "get_provisioning_status",
        package = PACKAGE_NAME,
        "init",
    );

    let public_key_path = data_dir.to_owned() + constants::CERT_PATH;
    let machine_cert_path = match construct_dir_path(&public_key_path) {
        Ok(v) => v,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "failed to construct machine cert path: {:?}, err - {:?}",
                public_key_path,
                e
            );
            bail!(e)
        }
    };
    let private_key_path = data_dir.to_owned() + constants::PRIVATE_KEY_PATH;
    let machine_private_key = match construct_dir_path(&private_key_path) {
        Ok(v) => v,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "failed to construct machine private key path: {:?}, err - {:?}",
                private_key_path,
                e
            );
            bail!(e)
        }
    };

    if machine_cert_path.exists() && machine_private_key.exists() {
        info!(
            func = fn_name,
            package = PACKAGE_NAME,
            "device is provisioned"
        );
        Ok(true)
    } else {
        info!(
            func = fn_name,
            package = PACKAGE_NAME,
            "device is not provisioned"
        );
        Ok(false)
    }
}
pub fn get_machine_id(data_dir: &str) -> Result<String> {
    trace!(func = "get_machine_id", "init");
    let public_key_path = data_dir.to_owned() + constants::CERT_PATH;
    let machine_id = match crypto::get_machine_id(&public_key_path) {
        Ok(v) => v,
        Err(e) => {
            error!(
                func = "get_machine_id",
                package = PACKAGE_NAME,
                "failed to get machine id: {:?}",
                e
            );
            bail!(e)
        }
    };
    Ok(machine_id)
}
pub fn get_machine_cert(data_dir: &str) -> Result<MachineCert> {
    trace!(func = "get_machine_cert", "init");
    let public_key_path = data_dir.to_owned() + constants::CERT_PATH;
    let ca_bundle_path = data_dir.to_owned() + constants::CA_BUNDLE_PATH;
    let root_cert_path = data_dir.to_owned() + constants::ROOT_CERT_PATH;
    let machine_cert =
        match crypto::get_machine_cert(&public_key_path, &ca_bundle_path, &root_cert_path) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    func = "get_machine_cert",
                    package = PACKAGE_NAME,
                    "failed to get machine cert: {:?}",
                    e
                );
                bail!(e)
            }
        };
    Ok(machine_cert)
}
pub fn get_machine_details(data_dir: &str) -> Result<MachineDetails> {
    let fn_name = "get_machine_details";
    trace!(func = "get_machine_details", "init");
    let public_key_path = data_dir.to_owned() + constants::CERT_PATH;
    let machine_id = match crypto::get_machine_id(&public_key_path) {
        Ok(machine_id) => machine_id,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "failed to get machine id: {:?}",
                e
            );
            bail!(e)
        }
    };

    let machine_cert = match get_machine_cert(data_dir) {
        Ok(machine_cert) => machine_cert,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "failed to get machine cert: {:?}",
                e
            );
            bail!(e)
        }
    };

    let data = MachineDetails {
        machine_id: machine_id.clone(),
        certificate_serial_number: machine_cert.serial_number.clone(),
        certificate_fingerprint: machine_cert.fingerprint.clone(),
    };
    Ok(data)
}
