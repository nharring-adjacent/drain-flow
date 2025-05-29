//! Generates realistic syslog messages, mimicking a diverse Linux system log (`/var/log/messages`).
// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use chrono::{DateTime, Duration, Utc};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng, seq::SliceRandom, distributions::Alphanumeric};
// use std::fmt::Write; // Not strictly needed with format! macro

pub struct SyslogEntry {
    pub timestamp: DateTime<Utc>,
    pub hostname: String,
    pub app_name: String,
    pub pid: Option<u32>,
    pub message: String,
}

pub fn format_syslog_entry(entry: &SyslogEntry) -> String {
    let timestamp_str = entry.timestamp.format("%b %d %H:%M:%S").to_string();
    let app_pid_str = match entry.pid {
        Some(pid) => format!("{}[{}]:", entry.app_name, pid),
        None => format!("{}:", entry.app_name),
    };
    format!("{} {} {} {}", timestamp_str, entry.hostname, app_pid_str, entry.message)
}

const CRON_USERS: &[&str] = &["root", "admin", "backup_user", "app_service", "ubuntu", "ec2-user"];
const CRON_COMMANDS: &[&str] = &[
    "/usr/local/bin/backup.sh --full",
    "python3 /opt/scripts/data_processing.py --mode=daily-summary",
    "curl -sS http://localhost/api/v1/health_check > /dev/null",
    "php /var/www/html/current/bin/console cron:run -q",
    "rsync -az --delete /data/production/ /data/backup_server::prod_backup/",
    "sudo -u www-data /usr/bin/certbot renew --quiet",
];

const SYSTEMD_SERVICES: &[&str] = &[
    "nginx.service", "postgresql@14-main.service", "redis-server.service", "docker.service",
    "cron.service", "systemd-networkd.service", "unattended-upgrades.service", "vector.service", "prometheus-node-exporter.service",
];
const SYSTEMD_TARGETS: &[&str] = &["multi-user.target", "graphical.target", "network-online.target", "cloud-init.target"];
const SYSTEMD_ACTIONS: &[&str] = &["Starting", "Started", "Stopping", "Stopped", "Reloading", "Reloaded"];


const SSH_USERS: &[&str] = &["alice", "bob", "dev_ops", "service_account_01", "jenkins", "ansible"];
const SSH_IPS: &[&str] = &["10.1.2.3", "192.168.1.100", "172.16.5.20", "203.0.113.45", "8.8.8.8"]; // Added a public IP for more variety
const SSH_RSA_FINGERPRINTS: &[&str] = &[
    "SHA256:abc123xyz789efg...", "SHA256:def456uvw456hij...", "SHA256:ghi789rst123klm...",
];

const KERNEL_MSG_TEMPLATES: &[&str] = &[
    "nf_conntrack: nf_conntrack: table full, dropping packet",
    "usb {usb_bus}-{usb_port}: new {usb_speed}-speed USB device number {device_num} using {usb_driver}",
    "CPU{cpu_id}: Performance Events: unsupported PEBS feature type 0x{hex_val} for generic counter",
    "audit: type=1400 audit({audit_ts}): apparmor=\"{status}\" operation=\"{op}\" profile=\"{profile}\" name=\"{name}\" pid={pid_val} comm=\"{comm}\" requested_mask=\"{req_mask}\" denied_mask=\"{den_mask}\" fsuid={fsuid} ouid={ouid}",
    "ata{ata_id}.00: failed command: {ata_cmd}",
    "systemd-journald[{pid_val}]: Received SIGTERM from PID 1 (systemd).",
    "random: crng init done. Initialized {bytes} bytes",
    "EXT4-fs ({device}): mounted filesystem with ordered data mode. Opts: (null)",
    "input: {input_device_name} as /devices/virtual/input/input{input_id}",
    "docker0: port {port_num}({veth_if}) entered blocking state",
    "docker0: port {port_num}({veth_if}) entered forwarding state",
];
const USB_BUS: &[&str] = &["1", "2", "3"];
const USB_PORT: &[&str] = &["1.1", "1.2", "2.1", "2.2", "3", "4"];
const USB_SPEED: &[&str] = &["high", "full", "super"];
const USB_DRIVER: &[&str] = &["xhci_hcd", "ehci-pci", "uhci_hcd"];
const ATA_ID: &[&str] = &["1", "2", "3", "4"];
const ATA_CMD: &[&str] = &["READ FPDMA QUEUED", "WRITE FPDMA QUEUED", "SET FEATURES", "IDENTIFY PACKET DEVICE"];
const INPUT_DEVICE_NAME: &[&str] = &["Power Button", "Sleep Button", "AT Translated Set 2 keyboard", "VirtualBox mouse integration"];
const VETH_IF_PREFIX: &[&str] = &["vethabc", "veth123", "vethxyz"];


const NTP_SERVERS: &[&str] = &["192.0.2.10", "203.0.113.25", "time.google.com", "pool.ntp.org", "0.arch.pool.ntp.org"];
const NTP_ACTIONS: &[&str] = &["synchronized to", "offset", "delay", "jitter", "poll"];

const DOMAINS: &[&str] = &["example.com", "internal.corp", "api.service.net", "backup.local", "google.com", "github.com"];
const RECORD_TYPES: &[&str] = &["A", "AAAA", "CNAME", "MX", "TXT", "SRV", "PTR"];

const POSTFIX_QUEUE_IDS: &[&str] = &["A1B2C3D4E5", "F6G7H8I9J0", "K1L2M3N4O5", "P6Q7R8S9T0", "U1V2W3X4Y5"];
const POSTFIX_EMAILS: &[&str] = &[
    "user1@example.com", "alert@monitoring.local", "backup_report@internal.corp", "test@test.com", "noreply@app.service.net",
];
const POSTFIX_RELAYS: &[&str] = &["mail.isp.com", "smtp.google.com", "internal-relay.corp", "127.0.0.1"];
const POSTFIX_STATUS: &[&str] = &["sent", "deferred", "bounced", "expired"];
const POSTFIX_DSN: &[&str] = &["2.0.0", "4.4.1", "5.1.1", "4.7.0", "5.7.1"];

const CUSTOM_SCRIPT_NAMES: &[&str] = &["data_backup.sh", "log_rotation.py", "health_monitor.pl", "deploy_app.rb"];
const CUSTOM_SCRIPT_IDS: &[&str] = &["job_123", "task_abc", "process_789", "item_xyz", "run_555"];
const CUSTOM_SCRIPT_ERRORS: &[&str] = &["E_NOENT", "E_ACCESS", "E_TIMEOUT", "E_CONFIG", "E_PERM", "E_IO"];
const CUSTOM_SCRIPT_VALUES: &[&str] = &["true", "false", "1024", "active", "pending", "completed", "failed"];

const SAMBA_SERVICES: &[&str] = &["shared_docs", "profiles", "backup_share", "public", "IPC$", "print$"];
const SAMBA_FUNCTIONS: &[&str] = &["close_cnum", "smbXsrv_session_logoff", "delete_path_default", "smb2_signing_check_pdu", "open_file"];

const NFSD_HOSTNAMES: &[&str] = &["client1.local", "appserver.corp", "backupclient.internal", "k8s-worker-01.cluster"];


fn generate_message_for_app(app_name: &str, rng: &mut StdRng, current_kernel_uptime_secs: f64, current_real_time: &DateTime<Utc>) -> String {
    match app_name {
        "CRON" => {
            let user = CRON_USERS.choose(rng).unwrap_or(&"root");
            let command = CRON_COMMANDS.choose(rng).unwrap_or(&"/bin/true");
            format!("({}) CMD ({})", user, command)
        }
        "systemd" => {
            if rng.gen_bool(0.8) { 
                let service = SYSTEMD_SERVICES.choose(rng).unwrap_or(&"unknown.service");
                let action = SYSTEMD_ACTIONS.choose(rng).unwrap_or(&"Starting");
                format!("{} {}...", action, service)
            } else {
                let target = SYSTEMD_TARGETS.choose(rng).unwrap_or(&"default.target");
                format!("Reached target {}.", target)
            }
        }
        "sshd" => {
            let action_type = rng.gen_range(0..5);
            let user = SSH_USERS.choose(rng).unwrap_or(&"unknown_user");
            let ip = SSH_IPS.choose(rng).unwrap_or(&"0.0.0.0");
            let port = rng.gen_range(10000..60000);
            match action_type {
                0 => {
                    let rsa_key = SSH_RSA_FINGERPRINTS.choose(rng).unwrap_or(&"key_fingerprint");
                    format!("Accepted publickey for {} from {} port {} ssh2: {}", user, ip, port, rsa_key)
                }
                1 => format!("pam_unix(sshd:session): session opened for user {} by (uid=0)", user),
                2 => format!("Received disconnect from {} port {}:11: disconnected by user", ip, port),
                3 => format!("Failed password for invalid user {} from {} port {} ssh2", user, ip, port),
                _ => format!("Connection closed by authenticating user {} {} port {}", user, ip, port),
            }
        }
        "kernel" => {
            let msg_template = KERNEL_MSG_TEMPLATES.choose(rng).unwrap_or(&"generic kernel message");
            let secs = current_kernel_uptime_secs.floor();
            let micros = (current_kernel_uptime_secs.fract() * 1_000_000.0).floor();
            let kernel_ts_str = format!("[{:>5}.{:06}]", secs as u64, micros as u32);

            // Replace placeholders in the chosen template
            let mut message = msg_template.to_string();
            message = message.replace("{usb_bus}", USB_BUS.choose(rng).unwrap_or(&"1"));
            message = message.replace("{usb_port}", USB_PORT.choose(rng).unwrap_or(&"1.1"));
            message = message.replace("{usb_speed}", USB_SPEED.choose(rng).unwrap_or(&"high"));
            message = message.replace("{device_num}", &rng.gen_range(2..10).to_string());
            message = message.replace("{usb_driver}", USB_DRIVER.choose(rng).unwrap_or(&"xhci_hcd"));
            message = message.replace("{cpu_id}", &rng.gen_range(0..4).to_string());
            message = message.replace("{hex_val}", &format!("{:x}", rng.gen_range(1..16)));
            message = message.replace("{audit_ts}", &current_real_time.timestamp_micros().to_string());
            message = message.replace("{status}", ["DENIED", "ALLOWED"].choose(rng).unwrap_or(&"DENIED"));
            message = message.replace("{op}", ["open", "connect", "mkdir"].choose(rng).unwrap_or(&"open"));
            message = message.replace("{profile}", ["/usr/sbin/sssd", "snap.docker.dockerd"].choose(rng).unwrap_or(&"/usr/sbin/sssd"));
            message = message.replace("{name}", ["/etc/krb5.keytab", "/var/log/messages"].choose(rng).unwrap_or(&"/etc/krb5.keytab"));
            message = message.replace("{pid_val}", &rng.gen_range(100..99999).to_string());
            message = message.replace("{comm}", ["sssd_be", "dockerd", "anacron"].choose(rng).unwrap_or(&"sssd_be"));
            message = message.replace("{req_mask}", ["r", "rw", "w"].choose(rng).unwrap_or(&"r"));
            message = message.replace("{den_mask}", ["r", "w"].choose(rng).unwrap_or(&"r"));
            message = message.replace("{fsuid}", &rng.gen_range(0..1000).to_string());
            message = message.replace("{ouid}", &rng.gen_range(0..1000).to_string());
            message = message.replace("{ata_id}", ATA_ID.choose(rng).unwrap_or(&"1"));
            message = message.replace("{ata_cmd}", ATA_CMD.choose(rng).unwrap_or(&"READ FPDMA QUEUED"));
            message = message.replace("{bytes}", &rng.gen_range(128..1024).to_string());
            message = message.replace("{device}", ["sda1", "nvme0n1p2", "vda"].choose(rng).unwrap_or(&"sda1"));
            message = message.replace("{input_device_name}", INPUT_DEVICE_NAME.choose(rng).unwrap_or(&"Unknown Input Device"));
            message = message.replace("{input_id}", &rng.gen_range(10..30).to_string());
            message = message.replace("{port_num}", &rng.gen_range(1..10).to_string());
            message = message.replace("{veth_if}", &format!("{}{:x}", VETH_IF_PREFIX.choose(rng).unwrap_or(&"veth"), rng.gen_range(100..999)));
            
            format!("{} {}", kernel_ts_str, message)
        }
        "ntpd" => {
            let action = NTP_ACTIONS.choose(rng).unwrap_or(&"synchronized to");
            match *action {
                "synchronized to" => {
                    let server_ip = NTP_SERVERS.choose(rng).unwrap_or(&"unknown.server");
                    let stratum = rng.gen_range(1..5);
                    format!("{} NTP server ({}) at stratum {}", action, server_ip, stratum)
                }
                "offset" => {
                    let offset_val = rng.gen_range(-0.1..0.1) as f64;
                    format!("{} {:.6} sec", action, offset_val)
                }
                _ => { // delay, jitter, poll
                    let val = rng.gen_range(0.001..0.5) as f64;
                    format!("{} {:.6} sec", action, val)
                }
            }
        }
        "named" => {
            let client_ip = SSH_IPS.choose(rng).unwrap_or(&"client.ip");
            let client_port = rng.gen_range(10000..60000);
            let domain_name = DOMAINS.choose(rng).unwrap_or(&"query.domain");
            let record_type = RECORD_TYPES.choose(rng).unwrap_or(&"A");
            let server_ip = NTP_SERVERS.choose(rng).unwrap_or(&"server.ip"); // Re-use for simplicity
            format!("client @0x{:x} {}#{} ({}): query: {} IN {} + ({})", 
                    rng.gen::<u32>(), client_ip, client_port, domain_name, domain_name, record_type, server_ip)
        }
        "postfix" => {
            let qid = POSTFIX_QUEUE_IDS.choose(rng).unwrap_or(&"NOQUEUE");
            let sub_type = rng.gen_range(0..3);
            match sub_type {
                0 => { // message-id
                    let msg_id_local_part = rng.gen::<u64>();
                    let msg_id_host = DOMAINS.choose(rng).unwrap_or(&"host.local");
                    format!("{}: message-id=<{}.{}@{}>", qid, current_real_time.timestamp_micros(), msg_id_local_part, msg_id_host)
                }
                1 => { // from, size, nrcpt
                    let from_email = POSTFIX_EMAILS.choose(rng).unwrap_or(&"null@local");
                    let size = rng.gen_range(100..50000);
                    let nrcpt = rng.gen_range(1..5);
                    format!("{}: from=<{}>, size={}, nrcpt={}", qid, from_email, size, nrcpt)
                }
                _ => { // to, relay, status
                    let to_email = POSTFIX_EMAILS.choose(rng).unwrap_or(&"recipient@remote");
                    let relay_srv = POSTFIX_RELAYS.choose(rng).unwrap_or(&"relay.host");
                    let relay_ip = SSH_IPS.choose(rng).unwrap_or(&"127.0.0.1"); // Can be an IP
                    let relay_port = [25, 587, 465].choose(rng).unwrap_or(&25);
                    let delay = rng.gen_range(0.1..15.0) as f32; // Total delay
                    let (d1, d2, d3, d4) = (delay*rng.gen_range(0.0..0.2), delay*rng.gen_range(0.1..0.3), delay*rng.gen_range(0.2..0.5), delay*rng.gen_range(0.3..0.7)); 
                    let dsn_val = POSTFIX_DSN.choose(rng).unwrap_or(&"2.0.0");
                    let status_val = POSTFIX_STATUS.choose(rng).unwrap_or(&"sent");
                    let reason_msg = if *status_val != "sent" { 
                        format!("(host {} said: {} {} - some_error_code)", relay_srv, rng.gen_range(400..599), POSTFIX_EMAILS.choose(rng).unwrap_or(&"unknown"))
                    } else { 
                        "250 2.0.0 OK".to_string()
                    };
                    format!("{}: to=<{}>, relay={}[{}]:{}, delay={:.1}, delays={:.1}/{:.1}/{:.1}/{:.1}, dsn={}, status={} ({})",
                            qid, to_email, relay_srv, relay_ip, relay_port, delay, d1, d2, d3, d4, dsn_val, status_val, reason_msg)
                }
            }
        }
        "custom_script" => {
            let script_name = CUSTOM_SCRIPT_NAMES.choose(rng).unwrap_or(&"generic.sh");
            let level = ["INFO", "WARN", "DEBUG", "ERROR"].choose(rng).unwrap_or(&"INFO");
            let item_id = CUSTOM_SCRIPT_IDS.choose(rng).unwrap_or(&"item_unknown");
            match *level {
                "INFO" => format!("{}: [{}] Processing item {} successfully.", level, script_name, item_id),
                "WARN" => {
                    let error = CUSTOM_SCRIPT_ERRORS.choose(rng).unwrap_or(&"E_UNKNOWN");
                    format!("{}: [{}] Item {} encountered warning: {}.", level, script_name, item_id, error)
                }
                "DEBUG" => {
                    let value = CUSTOM_SCRIPT_VALUES.choose(rng).unwrap_or(&"default_val");
                    let param = PARAM_KEYS.choose(rng).unwrap_or(&"config_param");
                    format!("{}: [{}] Parameter '{}' set to '{}' for item {}.", level, script_name, param, value, item_id)
                }
                _ => { // ERROR
                    let error = CUSTOM_SCRIPT_ERRORS.choose(rng).unwrap_or(&"E_CRITICAL");
                    format!("{}: [{}] Critical error for item {}: {}. Aborting operation.", level, script_name, item_id, error)
                }
            }
        }
        "samba" => {
            let samba_ts = current_real_time.format("%Y/%m/%d %H:%M:%S%.6f");
            let line_num = rng.gen_range(100..1000);
            let func_name = SAMBA_FUNCTIONS.choose(rng).unwrap_or(&"unknown_function");
            let client_ip = SSH_IPS.choose(rng).unwrap_or(&"samba.client.ip");
            let client_port = rng.gen_range(10000..60000);
            let service_name = SAMBA_SERVICES.choose(rng).unwrap_or(&"IPC$");
            let log_level = rng.gen_range(0..5);
            format!("[{}, {}] ../../source3/smbd/service.c:{}({})   {} (ipv4:{}:{}) closed connection to service {}",
                    samba_ts, log_level, line_num, func_name, client_ip, client_ip, client_port, service_name)
        }
        "nfsd" => {
            if rng.gen_bool(0.3) {
                "RPC: Dentry cache is full".to_string()
            } else if rng.gen_bool(0.3) {
                let client_host = NFSD_HOSTNAMES.choose(rng).unwrap_or(&"client.host");
                format!("lockd: server {} not responding, still trying", client_host)
            } else if rng.gen_bool(0.3) {
                let client_ip = SSH_IPS.choose(rng).unwrap_or(&"nfs.client.ip");
                let reason = ["access denied by server", "no_subtree_check", "sync error"].choose(rng).unwrap_or(&"unknown reason");
                format!("mountd: refused mount request from {} for /export/data ({})", client_ip, reason)
            }
             else {
                let auth_flavor = ["AUTH_NULL", "AUTH_SYS", "RPCSEC_GSS"].choose(rng).unwrap_or(&"AUTH_SYS");
                format!("auth: unhandled client flavor {} (status 0)", auth_flavor)
            }
        }
        "dockerd" => {
            let level = ["info", "warn", "error"].choose(rng).unwrap_or(&"info");
            let container_id_short: String = (0..12).map(|_| rng.sample(Alphanumeric) as char).collect::<String>().to_lowercase();
            let image_name = ["ubuntu:latest", "postgres:14-alpine", "nginx:1.21", "custom_app:v1.2.3"].choose(rng).unwrap_or(&"unknown_image");
            let action = ["start", "stop", "create", "destroy", "pull", "health_status"].choose(rng).unwrap_or(&"event");
            
            match *action {
                "start" => format!("level={} msg=\"Container {} ({}) started\" module=libcontainerd", level, container_id_short, image_name),
                "stop" => format!("level={} msg=\"Container {} ({}) stopped with exit code {}\" module=libcontainerd", level, container_id_short, image_name, rng.gen_range(0..=1)), // Inclusive range for 0 or 1
                "pull" => format!("level={} msg=\"Pulling fs layer\" image={} layer=fs{}", level, image_name, rng.gen_range(1..5)),
                "health_status" => format!("level={} msg=\"Health check failed\" container={} status=unhealthy error=\"timeout after 30s\"", level, container_id_short),
                _ => format!("level={} msg=\"API listen on /var/run/docker.sock\" module=daemon", level),
            }
        }
        "kubelet" => {
            let level_char = ["I", "W", "E", "F"].choose(rng).unwrap_or(&"I"); // Klog style
            let pod_name = format!("{}-{}-{}", ["frontend", "backend", "worker", "cache"].choose(rng).unwrap_or(&"app"), 
                                            ["blue", "green", "prod", "dev"].choose(rng).unwrap_or(&"prod"), 
                                            (0..5).map(|_| rng.sample(Alphanumeric) as char).collect::<String>().to_lowercase());
            let namespace = ["default", "kube-system", "production", "monitoring"].choose(rng).unwrap_or(&"default");
            let component = ["kubelet.go", "pleg.go", "volume_manager.go", "prober.go"].choose(rng).unwrap_or(&"kubelet.go");
            let line_num = rng.gen_range(100..2000);

            let event_type = ["SyncLoop", "PLEG", "Probe", "Eviction", "Config"].choose(rng).unwrap_or(&"Event");
            match *event_type {
                "SyncLoop" => format!("{} {}:{}:{}] \"{}\" pod=\"{}/{}\" status=\"{}\"", level_char, component, line_num, rng.gen_range(1..5), event_type, namespace, pod_name, ["running", "pending", "succeeded"].choose(rng).unwrap_or(&"running")),
                "PLEG" => format!("{} {}:{}:{}] \"{}\" Error: {}, Details: {}", level_char, component, line_num, rng.gen_range(1..5), event_type, ["FailedToGetContainerManager", "PodStopped"].choose(rng).unwrap_or(&"Error"), ["rpc error: code = DeadlineExceeded", "container not found"].choose(rng).unwrap_or(&"details")),
                "Probe" => format!("{} {}:{}:{}] \"{}\" pod=\"{}/{}\" probe=\"{}\" result=\"{}\"", level_char, component, line_num, rng.gen_range(1..5), event_type, namespace, pod_name, ["Readiness", "Liveness"].choose(rng).unwrap_or(&"Liveness"), ["Success", "Failure"].choose(rng).unwrap_or(&"Success")),
                _ => format!("{} {}:{}:{}] \"{}\" pod=\"{}/{}\" message=\"{}\"", level_char, component, line_num, rng.gen_range(1..5), event_type, namespace, pod_name, ["Configuration changed", "Pod sandbox changed"].choose(rng).unwrap_or(&"message")),
            }
        }
        _ => format!("Generic message for {} - ID: {}", app_name, rng.gen::<u32>()),
    }
}

// Constants for generate_syslog_messages
const HOSTNAMES: &[&str] = &["web01.example.com", "db01.internal.net", "utility-server03", "kube-node-05.cluster.local", "dev-vm01", "bastion.secure.corp"];
const APP_NAMES: &[&str] = &["CRON", "systemd", "sshd", "kernel", "ntpd", "named", "postfix", "custom_script", "samba", "nfsd", "dockerd", "kubelet", "vector-agent"];
const PARAM_KEYS: &[&str] = &["user_id", "session_key", "timeout", "retry_count", "debug_mode"]; // Used by custom_script

pub fn generate_syslog_messages(count: usize, seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut logs = Vec::with_capacity(count);
    
    // Start current_time some days in the past, ensures logs are not all from "now"
    let days_past = rng.gen_range(1..60);
    let mut current_time = Utc::now() - Duration::days(days_past);
    
    // Kernel's logical clock starts at some point before the first log message.
    // This offset represents how long the kernel has been "up" *before* the first log message we generate.
    let initial_kernel_uptime_offset_secs = rng.gen_range(0.0..(3_600_000.0 * 24.0 * 7.0)); // Up to 7 days of uptime before first log
    let mut current_kernel_uptime_secs = initial_kernel_uptime_offset_secs;

    for _i in 0..count {
        // Increment time for the main syslog timestamp
        let time_increment_secs = rng.gen_range(0..300); // 0 seconds to 5 minutes (0 for multiple logs at same time)
        let time_increment_nanos = rng.gen_range(0..1_000_000_000); // Add nanosecond precision
        current_time += Duration::seconds(time_increment_secs) + Duration::nanoseconds(time_increment_nanos);

        // Kernel's logical clock also moves forward by the same wall-clock amount
        current_kernel_uptime_secs += time_increment_secs as f64 + (time_increment_nanos as f64 / 1_000_000_000.0);

        let hostname = HOSTNAMES.choose(&mut rng).unwrap_or(&"localhost").to_string();
        let app_name = APP_NAMES.choose(&mut rng).unwrap_or(&"unknown_app").to_string();
        
        let pid = match app_name.as_str() {
            "kernel" => None,
            "systemd" if rng.gen_bool(0.1) => None, // Some systemd messages (like target reached) don't have PIDs
            _ => Some(rng.gen_range(100..99999)),
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

[end of benches/generators/syslog_gen.rs]
