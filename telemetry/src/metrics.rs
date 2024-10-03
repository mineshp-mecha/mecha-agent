use std::sync::RwLock;

use anyhow::{bail, Result};
use opentelemetry::{global, metrics::Meter, Key};
use sysinfo::{Disks, Networks, System};
use tracing::{error, info};

use crate::config::init_otlp_configuration;
const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
pub async fn initialize_metrics(otlp_endpoint_addr: String) -> Result<bool> {
    let fn_name = "initialize_custom_metrics";
    let _meter_provider = match init_otlp_configuration(otlp_endpoint_addr, 60) {
        Ok(provider) => {
            global::set_meter_provider(provider.clone());
            provider
        }
        Err(e) => {
            eprintln!("*********************** error initializing otlp configuration: {:?} ******************", e);
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error initializing otlp configuration: {:?}",
                e
            );
            return Ok(false);
        }
    };
    let meter = global::meter("ex.com/basic");
    match collect_memory_usage(meter.clone()) {
        Ok(_) => {
            info!(
                func = fn_name,
                package = PACKAGE_NAME,
                "memory utilization collected"
            )
        }
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error collecting memory utilization: {:?}",
                e
            );
            bail!("error collecting memory utilization: {:?}", e);
        }
    }
    match collect_cpu_utilization(meter.clone()) {
        Ok(_) => {
            info!(
                func = fn_name,
                package = PACKAGE_NAME,
                "cpu utilization collected"
            );
        }
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error collecting cpu utilization: {:?}",
                e
            );
            bail!("error collecting cpu utilization: {:?}", e);
        }
    }
    match collect_cpu_load_average(meter.clone()) {
        Ok(_) => {
            info!(
                func = fn_name,
                package = PACKAGE_NAME,
                "cpu load average collected"
            );
        }
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error collecting cpu load average: {:?}",
                e
            );
            bail!("error collecting cpu load average: {:?}", e);
        }
    }
    match collect_network_io(meter.clone()) {
        Ok(_) => {
            info!(
                func = fn_name,
                package = PACKAGE_NAME,
                "network io collected"
            );
        }
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error collecting network io: {:?}",
                e
            );
            bail!("error collecting network io: {:?}", e);
        }
    }
    match collect_disk_io(meter.clone()) {
        Ok(_) => {
            info!(func = fn_name, package = PACKAGE_NAME, "disk io collected");
        }
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error collecting disk io: {:?}",
                e
            );
            bail!("error collecting disk io: {:?}", e);
        }
    }
    match collect_filesystem_usage(meter.clone()) {
        Ok(_) => {
            info!(
                func = fn_name,
                package = PACKAGE_NAME,
                "filesystem usage collected"
            );
        }
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error collecting filesystem usage: {:?}",
                e
            );
            bail!("error collecting filesystem usage: {:?}", e);
        }
    }
    Ok(true)
}

// SYSTEM_CPU_UTILIZATION("system_cpu_time_seconds_total"),
fn collect_cpu_utilization(meter: Meter) -> Result<()> {
    let fn_name = "collect_cpu_utilization";
    let rw_guard = RwLock::new(System::new());
    meter
    .f64_observable_gauge("system.cpu.utilization")
    .with_description("Difference in system.cpu.time since the last measurement, divided by the elapsed time and number of logical CPUs")
    .with_unit("1")
    .with_callback(move |observer| {
        let mut sys = match rw_guard.write() {
            Ok(guard) => guard,
            Err(e) => {
                error!(
                    func = fn_name,
                    package = PACKAGE_NAME,
                    "error getting write lock: {:?}",
                    e
                );
                return;
            }
        };
        sys.refresh_cpu();
        for cpu in sys.cpus() {
            let attrs = vec![Key::new("system.cpu.logical_number").string(cpu.name().to_owned())];

            // total_cpu_usage += cpu.cpu_usage();
            observer.observe( cpu.cpu_usage() as f64, &attrs);
        }
    })
    .init();
    Ok(())
}

// SYSTEM_MEMORY_USAGE("system_memory_usage_bytes"),
fn collect_memory_usage(meter: Meter) -> Result<()> {
    meter
        .f64_observable_up_down_counter("system.memory.usage")
        .with_description("Reports memory in use by state.")
        .with_unit("By")
        .with_callback(|observer| {
            let guard = RwLock::new(System::new());
            let mut mem = match guard.write() {
                Ok(guard) => guard,
                Err(e) => {
                    error!("error getting write lock: {:?}", e);
                    return;
                }
            };
            mem.refresh_memory();
            let used_mem = mem.used_memory();
            let attrs_used = vec![Key::new("state").string("used")];
            observer.observe(used_mem as f64, &attrs_used);
            let attrs_free = vec![Key::new("state").string("free")];
            observer.observe(mem.free_memory() as f64, &attrs_free);
        })
        .init();
    Ok(())
}

//SYSTEM_CPU_LOAD_AVERAGE_15M("system_cpu_load_average_15m_ratio"),
fn collect_cpu_load_average(meter: Meter) -> Result<()> {
    meter
        .f64_observable_gauge("system.cpu.load_average.15m")
        .with_description("")
        .with_unit("1")
        .with_callback(|observer| {
            let load_avg = System::load_average().fifteen;
            let attrs = vec![];
            observer.observe(load_avg as f64, &attrs);
        })
        .init();
    Ok(())
}

// SYSTEM_NETWORK_IO("system_network_io_bytes_total"),
fn collect_network_io(meter: Meter) -> Result<()> {
    let fn_name = "collect_network_io";
    meter
        .f64_observable_counter("system.network.io")
        .with_description("")
        .with_unit("By")
        .with_callback(move |observer| {
            let rw_guard = RwLock::new(Networks::new_with_refreshed_list());
            let mut networks = match rw_guard.write() {
                Ok(guard) => guard,
                Err(e) => {
                    error!(
                        func = fn_name,
                        package = PACKAGE_NAME,
                        "error getting write lock: {:?}",
                        e
                    );
                    return;
                }
            };
            networks.refresh();
            for (interface_name, network) in networks.iter() {
                let total_transmitted_bytes = network.transmitted();
                let total_received_bytes = network.received();
                let attrs_transmit = vec![
                    Key::new("direction").string("transmit"),
                    Key::new("device").string(interface_name.to_owned()),
                ];
                observer.observe(total_transmitted_bytes as f64, &attrs_transmit);

                let attrs_receive = vec![
                    Key::new("direction").string("receive"),
                    Key::new("device").string(interface_name.to_owned()),
                ];
                observer.observe(total_received_bytes as f64, &attrs_receive);
            }
        })
        .init();
    Ok(())
}

// SYSTEM_DISK_IO("system_disk_io_bytes_total"),
fn collect_disk_io(meter: Meter) -> Result<()> {
    meter
        .f64_observable_counter("system.disk.io")
        .with_description("")
        .with_unit("By")
        .with_callback(|observer| {
            let s = System::new_all();

            for (_pid, process) in s.processes() {
                let read_bytes_data = process.disk_usage().read_bytes;
                let attrs_direction_read = vec![Key::new("direction").string("read")];
                observer.observe(read_bytes_data as f64, &attrs_direction_read);

                let write_bytes_data = process.disk_usage().written_bytes;
                let attrs_write = vec![Key::new("direction").string("write")];
                observer.observe(write_bytes_data as f64, &attrs_write);
            }
        })
        .init();
    Ok(())
}

// SYSTEM_FILESYSTEMS_USAGE("system_filesystem_usage_bytes");
fn collect_filesystem_usage(meter: Meter) -> Result<()> {
    let fn_name = "collect_filesystem_usage";
    meter
        .f64_observable_up_down_counter("system.filesystem.usage")
        .with_description("")
        .with_unit("By")
        .with_callback(move |observer| {
            let rw_guard = RwLock::new(Disks::new_with_refreshed_list());
            let mut disks = match rw_guard.write() {
                Ok(guard) => guard,
                Err(e) => {
                    error!(
                        func = fn_name,
                        package = PACKAGE_NAME,
                        "error getting write lock: {:?}",
                        e
                    );
                    return;
                }
            };
            disks.refresh();
            let mut used_space: u64 = 0;
            for disk in disks.list() {
                used_space += disk.total_space() - disk.available_space();
                let mount_point = disk.mount_point().to_owned();
                let mut attrs = vec![];
                attrs.push(Key::new("state").string("used"));
                attrs.push(Key::new("mountpoint").string(mount_point.to_str().unwrap().to_owned()));
                observer.observe(used_space as f64, &attrs);
            }
        })
        .init();
    Ok(())
}
