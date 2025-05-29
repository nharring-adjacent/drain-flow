// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

//! Generates realistic syslog messages, mimicking a diverse Linux system log (`/var/log/messages`).

use chrono::{DateTime, Duration, Utc};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rand::prelude::IndexedRandom; 

// Constants for generating varied log messages
const HOSTNAMES: &[&str] = &["primary-server", "backup-server", "app-vm-01", "db-node-3", "utility-box"];
const APP_NAMES: &[&str] = &[
    "CRON", "systemd", "sshd", "kernel", "ntpd", "named", 
    "postfix", "custom_script", "samba", "nfsd", "dockerd", "kubelet", "vector-agent"
];

const CRON_USERS: &[&str] = &["root", "admin", "backup_user", "app_service"];
const CRON_COMMANDS: &[&str] = &[
    "/usr/local/bin/backup_script.sh", 
    "command -v debian-sa1 > /dev/null && debian-sa1 1 1",
    "cd / && run-parts --report /etc/cron.hourly",
    "/opt/app/bin/cleanup_sessions"
];

const SYSTEMD_SERVICES: &[&str] = &["apache2.service", "mysql.service", "network.target", "docker.service", "cron.service", "unattended-upgrades.service"];
const SYSTEMD_ACTIONS: &[&str] = &["Starting", "Started", "Stopping", "Stopped", "Reached target", "Failed to start"];
const SYSTEMD_TARGETS: &[&str] = &["multi-user.target", "graphical.target", "network-online.target"];

const SSH_USERS: &[&str] = &["admin_user", "dev_ops", "service_account", "test_user", "invalid_user"];
const SSH_IPS: &[&str] = &["192.168.1.101", "10.0.2.15", "203.0.113.45", "172.17.0.1"];
const SSH_RSA_FINGERPRINTS: &[&str] = &[
    "SHA256:AbcDefGhiJklMnoPqrStuVwxYz1234567890abcDEF", 
    "SHA256:XyzAbcDefGhiJklMnoPqrStuVwxYz9876543210xyz"
];

const KERNEL_MSG_TEMPLATES: &[&str] = &[
    "nf_conntrack: nf_conntrack: table full, dropping packet",
    "usb {usb_bus}-{usb_port}: new {usb_speed}-speed USB device number {device_num} using {usb_driver}",
    "CPU{cpu_id}: Performance Events: unsupported PEBS feature type 0x{hex_val} for generic counter",
    "audit: type=1400 audit({ts_float}): apparmor=\"{status}\" operation=\"{op}\" profile=\"{profile}\" name=\"{name}\" pid={pid_val} comm=\"{comm}\" requested_mask=\"{req_mask}\" denied_mask=\"{den_mask}\" fsuid={fsuid} ouid={ouid}",
    "ata{ata_id}.00: CMD: {ata_cmd} {bytes} bytes",
    "random: crng init done",
    "EXT4-fs ({device}): mounted filesystem with ordered data mode. Opts: (null)",
    "input: {input_device_name} as /devices/virtual/input/input{input_id}",
    "{veth_if}: entered promiscuous mode",
    "docker0: port {port_num}({veth_if}) entered blocking state",
    "docker0: port {port_num}({veth_if}) entered forwarding state",
];
const USB_BUS: &[&str] = &["1", "2", "3"];
const USB_PORT: &[&str] = &["1.1", "1.2", "2.1", "3.4.1"];
const USB_SPEED: &[&str] = &["high", "full", "low"];
const USB_DRIVER: &[&str] = &["xhci_hcd", "ehci-pci", "ohci-pci"];
const ATA_ID: &[&str] = &["1", "2", "3", "4"];
const ATA_CMD: &[&str] = &["READ FPDMA QUEUED", "WRITE FPDMA QUEUED", "SET FEATURES"];
const INPUT_DEVICE_NAME: &[&str] = &["Power Button", "Sleep Button", "AT Translated Set 2 keyboard", "VirtualBox mouse integration"];

const NTP_ACTIONS: &[&str] = &["synchronized to", "offset", "delay", "jitter", "poll"];
const NTP_SERVERS: &[&str] = &["time.google.com", "pool.ntp.org", "192.168.1.1", "ntp.example.internal"];

const DOMAINS: &[&str] = &["example.com", "internal.lan", "google.com", "app.prod.local"];
const RECORD_TYPES: &[&str] = &["A", "AAAA", "MX", "CNAME", "TXT", "SRV"];

const POSTFIX_QUEUE_IDS: &[&str] = &["A1B2C3D4E5", "F6G7H8I9J0", "K1L2M3N4O5", "NOQUEUE"];
const POSTFIX_EMAILS: &[&str] = &["user@example.com", "admin@internal.lan", "test@test.com", "bounce@example.org"];
const POSTFIX_RELAYS: &[&str] = &["mail.isp.com", "smtp.google.com", "internal-relay.lan"];
const POSTFIX_DSN: &[&str] = &["2.0.0", "4.4.1", "5.1.1"];
const POSTFIX_STATUS: &[&str] = &["sent", "deferred", "bounced"];

const CUSTOM_SCRIPT_NAMES: &[&str] = &["backup.sh", "cleanup.py", "monitor_load.pl", "sync_data.rb"];
const CUSTOM_SCRIPT_IDS: &[&str] = &["task_123", "job_abc", "process_xyz", "item_789"];
const CUSTOM_SCRIPT_ERRORS: &[&str] = &["E_TIMEOUT", "E_DISK_FULL", "E_PERM_DENIED", "E_CONFIG_MISSING"];
const CUSTOM_SCRIPT_VALUES: &[&str] = &["true", "false", "0", "1024", "/mnt/data", "completed"];
const SYSLOG_PARAM_KEYS: &[&str] = &["service_status", "event_code", "user_id", "path", "duration_sec"];


const SAMBA_FUNCTIONS: &[&str] = &["close_cnum", "smbd_process", "make_connection_snum", "reply_tcon_and_X"];
const SAMBA_SERVICES: &[&str] = &["IPC$", "public_share", "homes", "print$"];

const NFSD_HOSTNAMES: &[&str] = &["client-A.lan", "compute-node-5.internal", "10.0.10.100"];

const DOCKERD_LEVELS: &[&str] = &["info", "warning", "error"];
const DOCKERD_CONTAINER_ID_PREFIX: &[&str] = &["a1b2c3d4e5f6", "7g8h9i0j1k2l", "m3n4o5p6q7r8s9"];
const DOCKERD_IMAGE_NAMES: &[&str] = &["ubuntu:latest", "postgres:14-alpine", "nginx:1.21", "custom_app:v1.2.3"];
const DOCKERD_ACTIONS: &[&str] = &["start", "stop", "create", "destroy", "pull", "health_status"];

const KUBELET_POD_NAMES: &[&str] = &["frontend-", "backend-", "database-", "worker-", "cache-"];
const KUBELET_NAMESPACES: &[&str] = &["default", "kube-system", "production", "monitoring", "dev"];
const KUBELET_COMPONENTS: &[&str] = &["kubelet.go", "pleg.go", "volume_manager.go", "prober.go", "server.go"];
const KUBELET_EVENT_TYPES: &[&str] = &["SyncLoop", "PLEG", "Probe", "Eviction", "Config"];
const KUBELET_PROBE_TYPES: &[&str] = &["Readiness", "Liveness"];
const KUBELET_PROBE_RESULTS: &[&str] = &["Success", "Failure"];
const KUBELET_STATUS: &[&str] = &["running", "pending", "succeeded", "failed", "unknown"];

const CHARSET: &[u8] = b"abcdef0123456789"; 
const VETH_IF_PREFIX: &[&str] = &["veth", "vetheth"]; 

#[derive(Debug)]
pub struct SyslogEntry {
    pub timestamp: chrono::DateTime<Utc>,
    pub hostname: String,
    pub app_name: String,
    pub pid: Option<u32>,
    pub message: String,
}

pub fn format_syslog_entry(entry: &SyslogEntry) -> String {
    let ts_str = entry.timestamp.format("%b %d %H:%M:%S").to_string();
    let app_pid_str = if let Some(p) = entry.pid {
        format!("{}[{}]:", entry.app_name, p)
    } else {
        format!("{}:", entry.app_name)
    };
    format!("{} {} {} {}", ts_str, entry.hostname, app_pid_str, entry.message)
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
fn generate_message_for_app(app_name: &str, rng: &mut StdRng, kernel_uptime_secs: f64, current_wall_time: &DateTime<Utc>) -> String {
    match app_name {
        "CRON" => {
            let user = CRON_USERS.choose(rng).unwrap_or(&"root");
            let command = CRON_COMMANDS.choose(rng).unwrap_or(&"/bin/true");
            format!("({}) CMD ({})", user, command)
        }
        "systemd" => {
            if rng.random_bool(0.8) { 
                let service = SYSTEMD_SERVICES.choose(rng).unwrap_or(&"unknown.service");
                let action = SYSTEMD_ACTIONS.choose(rng).unwrap_or(&"Starting");
                format!("{} {}...", action, service)
            } else { 
                let target = SYSTEMD_TARGETS.choose(rng).unwrap_or(&"default.target");
                format!("Reached target {}.", target)
            }
        }
        "sshd" => {
            let action_type = rng.random_range(0..5);
            let user = SSH_USERS.choose(rng).unwrap_or(&"unknown_user");
            let ip = SSH_IPS.choose(rng).unwrap_or(&"0.0.0.0");
            let port = rng.random_range(10000..60000);
            match action_type {
                0 => format!("Accepted publickey for {} from {} port {} ssh2: RSA {}", user, ip, port, SSH_RSA_FINGERPRINTS.choose(rng).unwrap_or(&"key_fingerprint")),
                1 => format!("pam_unix(sshd:session): session opened for user {} by (uid=0)", user),
                2 => format!("Received disconnect from {} port {}:11: disconnected by user", ip, port),
                3 => format!("Failed password for {} user {} from {} port {} ssh2", if rng.random_bool(0.5) {"invalid"} else {""}, user, ip, port),
                _ => format!("Connection closed by authenticating user {} {} port {}", user, ip, port),
            }
        }
        "kernel" => {
            let uptime_micros = (kernel_uptime_secs * 1_000_000.0) as u64 + rng.random_range(0..1_000_000);
            let uptime_secs_display = uptime_micros / 1_000_000;
            let uptime_frac_display = uptime_micros % 1_000_000;
            let base_msg = format!("[{:5}.{:06}] ", uptime_secs_display, uptime_frac_display);
            
            let mut message = KERNEL_MSG_TEMPLATES.choose(rng).unwrap_or(&"generic kernel message").to_string();
            message = message.replace("{usb_bus}", USB_BUS.choose(rng).unwrap_or(&"1"));
            message = message.replace("{usb_port}", USB_PORT.choose(rng).unwrap_or(&"1.1"));
            message = message.replace("{usb_speed}", USB_SPEED.choose(rng).unwrap_or(&"high"));
            message = message.replace("{device_num}", &rng.random_range(2..10).to_string());
            message = message.replace("{usb_driver}", USB_DRIVER.choose(rng).unwrap_or(&"xhci_hcd"));
            message = message.replace("{cpu_id}", &rng.random_range(0..4).to_string());
            message = message.replace("{hex_val}", &format!("{:x}", rng.random_range(1..16)));
            message = message.replace("{ts_float}", &format!("{}.{}", current_wall_time.timestamp(), current_wall_time.timestamp_subsec_micros()));
            message = message.replace("{status}", if *(*["DENIED", "ALLOWED"].choose(rng).unwrap_or(&"DENIED")) == *"DENIED" {"DENIED"} else {"ALLOWED"}); // Corrected comparison
            message = message.replace("{op}", *["open", "connect", "mkdir"].choose(rng).unwrap_or(&"open"));
            message = message.replace("{profile}", *["/usr/sbin/sssd", "snap.docker.dockerd"].choose(rng).unwrap_or(&"/usr/sbin/sssd"));
            message = message.replace("{name}", *["/etc/krb5.keytab", "/var/log/messages"].choose(rng).unwrap_or(&"/etc/krb5.keytab"));
            message = message.replace("{pid_val}", &rng.random_range(100..99999).to_string());
            message = message.replace("{comm}", *["sssd_be", "dockerd", "anacron"].choose(rng).unwrap_or(&"sssd_be"));
            message = message.replace("{req_mask}", *["r", "rw", "w"].choose(rng).unwrap_or(&"r"));
            message = message.replace("{den_mask}", *["r", "w"].choose(rng).unwrap_or(&"r"));
            message = message.replace("{fsuid}", &rng.random_range(0..1000).to_string());
            message = message.replace("{ouid}", &rng.random_range(0..1000).to_string());
            message = message.replace("{ata_id}", ATA_ID.choose(rng).unwrap_or(&"1"));
            message = message.replace("{ata_cmd}", ATA_CMD.choose(rng).unwrap_or(&"READ FPDMA QUEUED"));
            message = message.replace("{bytes}", &rng.random_range(128..1024).to_string());
            message = message.replace("{device}", *["sda1", "nvme0n1p2", "vda"].choose(rng).unwrap_or(&"sda1"));
            message = message.replace("{input_device_name}", INPUT_DEVICE_NAME.choose(rng).unwrap_or(&"Unknown Input Device"));
            message = message.replace("{input_id}", &rng.random_range(10..30).to_string());
            message = message.replace("{port_num}", &rng.random_range(1..10).to_string());
            let veth_prefix = VETH_IF_PREFIX.choose(rng).unwrap_or(&"veth");
            let veth_suffix: String = (0..6).map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char).collect();
            message = message.replace("{veth_if}", &format!("{}{}", veth_prefix, veth_suffix));

            format!("{}{}", base_msg, message)
        }
        "ntpd" => {
            let action_ref = NTP_ACTIONS.choose(rng).unwrap_or(&"synchronized to");
            match *action_ref { 
                "synchronized to" => {
                    let server_ip = NTP_SERVERS.choose(rng).unwrap_or(&"unknown.server");
                    let stratum = rng.random_range(1..5);
                    format!("{} NTP server ({}) at stratum {}", action_ref, server_ip, stratum)
                }
                "offset" | "delay" | "jitter" | "poll" => {
                    let val = rng.random_range(0.001..0.5) as f64;
                    format!("{} {:.6} sec", action_ref, val)
                }
                _ => format!("{} some_value", action_ref),
            }
        }
        "named" => {
            let client_ip = SSH_IPS.choose(rng).unwrap_or(&"client.ip"); 
            let client_port = rng.random_range(10000..60000);
            let domain_name = DOMAINS.choose(rng).unwrap_or(&"query.domain");
            let record_type = RECORD_TYPES.choose(rng).unwrap_or(&"A");
            let server_ip = NTP_SERVERS.choose(rng).unwrap_or(&"server.ip"); 
            format!("client @0x{:x} {}#{} ({}): query: {} IN {} + ({})", 
                rng.random::<u32>(), client_ip, client_port, domain_name, domain_name, record_type, server_ip)
        }
        "postfix" => {
            let qid = POSTFIX_QUEUE_IDS.choose(rng).unwrap_or(&"NOQUEUE");
            let sub_type = rng.random_range(0..3);
            match sub_type {
                0 => { 
                    let msg_id_local_part = rng.random::<u64>();
                    let msg_id_host = DOMAINS.choose(rng).unwrap_or(&"host.local");
                    format!("{}: message-id=<{:x}.{:x}@{}>", qid, msg_id_local_part, rng.random::<u32>(), msg_id_host)
                }
                1 => { 
                    let from_email = POSTFIX_EMAILS.choose(rng).unwrap_or(&"null@local");
                    let size = rng.random_range(100..50000);
                    let nrcpt = rng.random_range(1..5);
                    format!("{}: from=<{}>, size={}, nrcpt={}", qid, from_email, size, nrcpt)
                }
                _ => { 
                    let to_email = POSTFIX_EMAILS.choose(rng).unwrap_or(&"recipient@remote");
                    let relay_srv = POSTFIX_RELAYS.choose(rng).unwrap_or(&"relay.host");
                    let relay_ip = SSH_IPS.choose(rng).unwrap_or(&"127.0.0.1"); 
                    let relay_port = *[25, 587, 465].choose(rng).unwrap_or(&25);
                    let delay = rng.random_range(0.1..15.0) as f32; 
                    let (d1, d2, d3, d4) = (delay*rng.random_range(0.0..0.2), delay*rng.random_range(0.1..0.3), delay*rng.random_range(0.2..0.5), delay*rng.random_range(0.3..0.7)); 
                    let dsn_val = POSTFIX_DSN.choose(rng).unwrap_or(&"2.0.0");
                    let status_val_ref = POSTFIX_STATUS.choose(rng).unwrap_or(&"sent");
                    let reason = if *status_val_ref != "sent" { 
                        format!("(host {} said: {} {} - some_error_code)", relay_srv, rng.random_range(400..599), POSTFIX_EMAILS.choose(rng).unwrap_or(&"unknown"))
                    } else { 
                        "message accepted for delivery".to_string() 
                    };
                    format!("{}: to=<{}>, relay={}[{}]:{}, delay={:.1}, delays={:.1}/{:.1}/{:.1}/{:.1}, dsn={}, status={} ({})",
                        qid, to_email, relay_srv, relay_ip, relay_port, delay, d1, d2, d3, d4, dsn_val, status_val_ref, reason)
                }
            }
        }
        "custom_script" => {
            let script_name = CUSTOM_SCRIPT_NAMES.choose(rng).unwrap_or(&"generic.sh");
            let level_ref = ["INFO", "WARN", "DEBUG", "ERROR"].choose(rng).unwrap_or(&"INFO"); 
            let item_id = CUSTOM_SCRIPT_IDS.choose(rng).unwrap_or(&"item_unknown");
            match *level_ref { 
                "INFO" => format!("{}: [{}] Processing item {}.", level_ref, script_name, item_id),
                "WARN" => format!("{}: [{}] Item {} has a minor issue.", level_ref, script_name, item_id),
                "DEBUG" => {
                    let param = SYSLOG_PARAM_KEYS.choose(rng).unwrap_or(&"config_param"); 
                    let value = CUSTOM_SCRIPT_VALUES.choose(rng).unwrap_or(&"default_val");
                    format!("{}: [{}] Value of {} set to {}.", level_ref, script_name, param, value)
                },
                _ => { 
                    let error = CUSTOM_SCRIPT_ERRORS.choose(rng).unwrap_or(&"E_UNKNOWN");
                    format!("{}: [{}] Item {} failed with error: {}.", level_ref, script_name, item_id, error)
                }
            }
        }
        "samba" => {
            let samba_ts = current_wall_time.format("%Y/%m/%d %H:%M:%S%.6f").to_string();
            let line_num = rng.random_range(100..1000);
            let func_name = SAMBA_FUNCTIONS.choose(rng).unwrap_or(&"unknown_function");
            let client_ip = SSH_IPS.choose(rng).unwrap_or(&"samba.client.ip");
            let client_port = rng.random_range(10000..60000);
            let service_name = SAMBA_SERVICES.choose(rng).unwrap_or(&"IPC$");
            let log_level = rng.random_range(0..5);
            format!("[{},  {}] ../../source3/smbd/service.c:{}({})   {} (ipv4:{}:{}) closed connection to service {}",
                samba_ts, log_level, line_num, func_name, client_ip, client_ip, client_port, service_name)
        }
        "nfsd" => {
            if rng.random_bool(0.3) {
                format!("RPC: Dentry cache is full")
            } else if rng.random_bool(0.3) {
                let client_host = NFSD_HOSTNAMES.choose(rng).unwrap_or(&"client.host");
                format!("lockd: server {} not responding, still trying", client_host)
            } else if rng.random_bool(0.3) {
                let client_ip = SSH_IPS.choose(rng).unwrap_or(&"nfs.client.ip");
                let reason = *["access denied by server", "no_subtree_check", "sync error"].choose(rng).unwrap_or(&"unknown reason");
                format!("mountd: refused mount request from {} for /exports/data ({})", client_ip, reason)
            } else {
                let auth_flavor = *["AUTH_NULL", "AUTH_SYS", "RPCSEC_GSS"].choose(rng).unwrap_or(&"AUTH_SYS");
                format!("auth: unhandled client flavor {}", auth_flavor)
            }
        }
        "dockerd" => {
            let level = DOCKERD_LEVELS.choose(rng).unwrap_or(&"info");
            let container_id_short: String = (0..12).map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char).collect();
            let image_name = DOCKERD_IMAGE_NAMES.choose(rng).unwrap_or(&"unknown_image");
            let action_ref = DOCKERD_ACTIONS.choose(rng).unwrap_or(&"event");
            
            match *action_ref { 
                "start" | "stop" | "create" | "destroy" => format!("level={} msg=\"Container {} {} {} ({})\"", level, container_id_short, action_ref, image_name, DOCKERD_CONTAINER_ID_PREFIX.choose(rng).unwrap_or(&"abcdef")),
                "pull" => format!("level={} msg=\"Pulling fs layer\" image={} layer=fs{}", level, image_name, rng.random_range(1..5)),
                _ => format!("level={} msg=\"Health status for container {} is {}\" module=libcontainerd status={}", level, container_id_short, image_name, rng.random_range(0..=1)),
            }
        }
        "kubelet" => {
            let level_char = ["I", "W", "E", "F"].choose(rng).unwrap_or(&"I"); 
            let klog_ts = current_wall_time.format("%m%d %H:%M:%S.%f").to_string();
            let thread_id = 1; 
            
            let pod_name_prefix = KUBELET_POD_NAMES.choose(rng).unwrap_or(&"app-");
            let pod_suffix: String = (0..5).map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char).collect();
            let pod_name = format!("{}{}", pod_name_prefix, pod_suffix);
            let namespace = KUBELET_NAMESPACES.choose(rng).unwrap_or(&"default");
            let component = KUBELET_COMPONENTS.choose(rng).unwrap_or(&"kubelet.go");
            let line_num = rng.random_range(100..2000);
            
            let event_type_ref = KUBELET_EVENT_TYPES.choose(rng).unwrap_or(&"Event");
            let message_body = match *event_type_ref { 
                "SyncLoop" => format!("\"{}\" pod=\"{}/{}\" status=\"{}\"", event_type_ref, namespace, pod_name, KUBELET_STATUS.choose(rng).unwrap_or(&"running")),
                "PLEG" => format!("\"{}\" Error: {}, Details: {}", event_type_ref, (*["FailedToGetContainerManager", "PodStopped"].choose(rng).unwrap_or(&"Error")), (*["rpc error: code = DeadlineExceeded", "container not found"].choose(rng).unwrap_or(&"details"))),
                "Probe" => format!("\"{}\" pod=\"{}/{}\" probe=\"{}\" result=\"{}\"", event_type_ref, namespace, pod_name, KUBELET_PROBE_TYPES.choose(rng).unwrap_or(&"Liveness"), KUBELET_PROBE_RESULTS.choose(rng).unwrap_or(&"Success")),
                _ => format!("\"{}\" pod=\"{}/{}\" message=\"{}\"", event_type_ref, namespace, pod_name, (*["Configuration changed", "Pod sandbox changed"].choose(rng).unwrap_or(&"message"))),
            };
            format!("{} {} {:>7} {}:{}] {}", level_char, klog_ts, thread_id, component, line_num, message_body)
        }
        "vector-agent" => format!("INFO vector::shutdown: Vector has stopped."),
        _ => format!("Generic message for {} - ID: {}", app_name, rng.random::<u32>()),
    }
}

pub fn generate_syslog_messages(count: usize, seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut logs = Vec::with_capacity(count);
    
    let days_past = rng.random_range(1..60);
    let mut current_time = Utc::now() - Duration::days(days_past);
    let initial_kernel_uptime_offset_secs = rng.random_range(0.0..(3_600_000.0 * 24.0 * 7.0)); 
    let mut current_kernel_uptime_secs = initial_kernel_uptime_offset_secs;

    for _ in 0..count {
        let time_increment_secs = rng.random_range(0..300); 
        let time_increment_nanos = rng.random_range(0..1_000_000_000); 
        current_time += Duration::seconds(time_increment_secs) + Duration::nanoseconds(time_increment_nanos);
        
        current_kernel_uptime_secs += rng.random_range(0.0..5.0) + (rng.random_range(0..1_000_000) as f64 / 1_000_000.0);

        let hostname = HOSTNAMES.choose(&mut rng).unwrap_or(&"localhost").to_string();
        let app_name = APP_NAMES.choose(&mut rng).unwrap_or(&"unknown_app").to_string();
        
        let pid = match app_name.as_str() {
            "kernel" => None,
            "systemd" if rng.random_bool(0.1) => None, 
            _ => Some(rng.random_range(100..99999)),
        };

        let message = generate_message_for_app(&app_name, &mut rng, current_kernel_uptime_secs, &current_time);

        let entry = SyslogEntry {
            timestamp: current_time,
            hostname,
            app_name,
            pid,
            message,
        };
        logs.push(format_syslog_entry(&entry));
    }
    logs
}

