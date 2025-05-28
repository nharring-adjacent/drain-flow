//! Generates realistic klog-formatted logs for Kubernetes infrastructure components (e.g., Kubelet, API server).
// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use rand::{Rng, SeedableRng, seq::SliceRandom, rngs::StdRng};
use chrono::{Utc, DateTime, Duration};
use std::fmt::{Display, Formatter, Result as FmtResult};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,    // I
    Warning, // W
    Error,   // E
    Fatal,   // F
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            LogLevel::Info => write!(f, "I"),
            LogLevel::Warning => write!(f, "W"),
            LogLevel::Error => write!(f, "E"),
            LogLevel::Fatal => write!(f, "F"),
        }
    }
}

#[derive(Debug)]
pub struct K8sObjectRef {
    kind: String,
    namespace: String,
    name: String,
    uid: String,
}

impl K8sObjectRef {
    fn new(kind: &str, namespace: &str, name: &str, rng: &mut StdRng) -> Self {
        K8sObjectRef {
            kind: kind.to_string(),
            namespace: namespace.to_string(),
            name: name.to_string(),
            uid: Uuid::new_v4().to_string(),
        }
    }

    fn to_log_string(&self) -> String {
        format!("v1.ObjectReference{{Kind:\"{}\", Namespace:\"{}\", Name:\"{}\", UID:\"{}\", APIVersion:\"v1\"}}",
                self.kind, self.namespace, self.name, self.uid)
    }

    fn to_namespaced_name(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}


pub fn format_klog_entry(
    level: LogLevel,
    timestamp: &DateTime<Utc>,
    thread_id: u32,
    file_line: &str,
    message: &str,
) -> String {
    format!(
        "{}{} {:7} {}] {}", // Padded thread_id to 7 chars
        level,
        timestamp.format("%m%d %H:%M:%S.%f"),
        thread_id,
        file_line,
        message
    )
}

const KUBELET_FILES: &[&str] = &["kubelet.go:1234", "pleg.go:567", "volume_manager.go:890", "oom_watcher.go:78", "container_manager_linux.go:345", "server.go:432", "status_manager.go:221", "prober.go:112"];
const APISERVER_FILES: &[&str] = &["storage_rbac.go:321", "generic_apiserver.go:789", "authentication.go:101", "audit.go:234", "etcd_storage.go:567", "watch.go:890", "admission.go:123"];
const SCHEDULER_FILES: &[&str] = &["scheduler.go:234", "core_scheduler.go:567", "priorities.go:890", "factory.go:123", "generic_scheduler.go:456", "metrics.go:789"];
const ETCD_FILES: &[&str] = &["server.go:123", "raft.go:456", "mvcc_db.go:789", "cluster.go:234", "wal.go:567", "auth.go:890"];
const CONTROLLER_MANAGER_FILES: &[&str] = &["node_controller.go:101", "deployment_controller.go:202", "serviceaccount_controller.go:303", "replicaset_controller.go:404", "cronjob_controller.go:505", "garbagecollector_controller.go:606"];
const KUBE_PROXY_FILES: &[&str] = &["proxier.go:404", "service.go:505", "endpoints.go:606", "healthcheck.go:707", "iptables.go:808"];

const POD_NAMES: &[&str] = &["frontend-app", "backend-worker", "database-master", "cache-node", "monitoring-agent", "logging-sidecar"];
const NAMESPACES: &[&str] = &["default", "kube-system", "production", "staging", "dev-tools", "monitoring-ns"];
const NODE_NAMES: &[&str] = &["node-01-worker", "node-02-master", "node-03-storage", "ip-10-0-1-100.ec2.internal", "aks-nodepool1-12345678-vmss000000"];
const CONTAINER_IMAGE_NAMES: &[&str] = &["nginx:1.21", "postgres:14-alpine", "custom-app:v1.2.3", "redis:6.0", "prom/prometheus:v2.30.0", "grafana/grafana:8.2.0"];
const K8S_KINDS: &[&str] = &["Pod", "Deployment", "Service", "Node", "ConfigMap", "Secret", "PersistentVolumeClaim", "Namespace"];
const ERROR_MESSAGES: &[&str] = &["context deadline exceeded", "resource temporarily unavailable", "connection refused", "no such host", "permission denied", "image pull failed", "invalid configuration", "network is unreachable", "etcdserver: request timed out"];
const USERS: &[&str] = &["system:serviceaccount:kube-system:kubelet", "system:kube-scheduler", "admin@example.com", "system:node:node-01-worker", "kube-controller-manager"];
const GROUPS: &[&str] = &["system:nodes", "system:authenticated", "system:masters", "system:serviceaccounts"];
const WEBHOOK_NAMES: &[&str] = &["validation.example.com", "mutation.k8s.io", "kyverno-policy-webhook"];
const CONTROLLER_NAMES: &[&str] = &["Deployment", "ReplicaSet", "Node", "ServiceAccount", "CronJob", "StatefulSet", "PersistentVolumeClaim"];


fn generate_k8s_object(kind: Option<&str>, ns: Option<&str>, name_prefix: Option<&str>, rng: &mut StdRng) -> K8sObjectRef {
    let actual_kind = kind.unwrap_or_else(|| K8S_KINDS.choose(rng).unwrap_or(&"Pod"));
    let actual_ns = ns.unwrap_or_else(|| NAMESPACES.choose(rng).unwrap_or(&"default"));
    let actual_name = format!("{}-{}",
        name_prefix.unwrap_or_else(|| POD_NAMES.choose(rng).unwrap_or(&"app")),
        rng.gen_range(10000..99999)
    );
    K8sObjectRef::new(actual_kind, actual_ns, &actual_name, rng)
}


fn generate_message_for_component(component: &str, level: LogLevel, rng: &mut StdRng) -> String {
    let pod_obj = generate_k8s_object(Some("Pod"), None, None, rng);
    let node_name = NODE_NAMES.choose(rng).unwrap_or(&"node-unknown");
    let image_name = CONTAINER_IMAGE_NAMES.choose(rng).unwrap_or(&"unknown-image");
    let error_msg = ERROR_MESSAGES.choose(rng).unwrap_or(&"unknown error");
    let user = USERS.choose(rng).unwrap_or(&"system:unknown");
    let groups_vec: Vec<&&str> = GROUPS.iter().filter(|_| rng.gen_bool(0.3)).collect(); // Select some groups
    let groups_str = groups_vec.iter().map(|s| s.to_string()).collect::<Vec<String>>().join(",");
    let webhook_name = WEBHOOK_NAMES.choose(rng).unwrap_or(&"unknown-webhook");
    let controller_name = CONTROLLER_NAMES.choose(rng).unwrap_or(&"UnknownController");
    let item_key = generate_k8s_object(None, None, None, rng).to_namespaced_name();


    match component {
        "kubelet" => match level {
            LogLevel::Info => {
                let event_type = ["Normal", "SuccessfulCreate", "SuccessfulMountVolume"].choose(rng).unwrap_or(&"Normal");
                let reason = ["Scheduled", "Pulled", "Created", "Started", "Killing", "AddedInterface", "VolumeMounted"].choose(rng).unwrap_or(&"GenericReason");
                let messages = [
                    format!("Event({}): type: '{}' reason: '{}' Successfully assigned {} to {}", pod_obj.to_log_string(), event_type, reason, pod_obj.to_namespaced_name(), node_name),
                    format!("SyncLoop (ADD, \"api\"): \"{}_{}_{}\"", pod_obj.uid, pod_obj.namespace, pod_obj.name),
                    format!("\"Adding pod to network\" pod=\"{}\"", pod_obj.to_namespaced_name()),
                    format!("\"Updating status for pod\" pod=\"{}\" status={{phase: \"Running\", conditions: [...]}}", pod_obj.to_namespaced_name()),
                    format!("\"Starting kubelet\" version=\"v1.25.3\" node=\"{}\"", node_name),
                    format!("\"Image GC completed\" images_deleted={} bytes_reclaimed={}", rng.gen_range(0..5), rng.gen_range(0..1024*1024*500)),
                    format!("\"PLEG: pod final state arrived\" podID=\"{}\"", pod_obj.uid),
                ];
                messages.choose(rng).unwrap_or(&"Default Kubelet Info".to_string()).to_string()
            }
            LogLevel::Warning | LogLevel::Error => {
                let messages = [
                    format!("\"Failed to pull image\" image=\"{}\" err=\"{}\"", image_name, error_msg),
                    format!("\"Pod sync failed\" pod=\"{}\" err=\"{}\"", pod_obj.to_namespaced_name(), error_msg),
                    format!("\"Failed to update node status\" err=\"{}\"", error_msg),
                    format!("\"Container runtime is down\" err=\"{}\"", error_msg),
                    format!("\"Failed to delete pod\" pod=\"{}\" err=\"Failed to stop container {} with error: {}\"", pod_obj.to_namespaced_name(), image_name, error_msg),
                    format!("\"Evicting pod due to MemoryPressure\" pod=\"{}\" node=\"{}\"", pod_obj.to_namespaced_name(), node_name),
                ];
                messages.choose(rng).unwrap_or(&"Default Kubelet Error".to_string()).to_string()
            }
            LogLevel::Fatal => format!("\"Kubelet process exiting\" reason=\"{}\"", error_msg),
        },
        "kube-apiserver" => match level {
            LogLevel::Info => {
                let method = ["GET", "POST", "PUT", "DELETE"].choose(rng).unwrap_or(&"GET");
                let path = format!("/api/v1/namespaces/{}/pods/{}", pod_obj.namespace, pod_obj.name);
                let event = ["ADDED", "MODIFIED", "DELETED"].choose(rng).unwrap_or(&"ADDED");
                let messages = [
                    format!("\"Handling request\" method=\"{}\" path=\"{}\" user=\"{}\" groups=\"{}\" clientIP=\"10.1.2.3:12345\"", method, path, user, groups_str),
                    format!("\"Resource event\" kind=\"{}\" event=\"{}\" name=\"{}\" namespace=\"{}\"", pod_obj.kind, event, pod_obj.name, pod_obj.namespace),
                    format!("\"Starting Kubernetes API server\" version=\"v1.25.3\""),
                    format!("\"Successfully initialized admission plugin\" name=\"{}\"", webhook_name),
                ];
                messages.choose(rng).unwrap_or(&"Default API Server Info".to_string()).to_string()
            }
            LogLevel::Warning | LogLevel::Error => {
                let kind = K8S_KINDS.choose(rng).unwrap_or(&"Deployment");
                let name = generate_k8s_object(Some(kind), None, None, rng).name;
                let messages = [
                    format!("\"Admission webhook denied request\" webhook=\"{}\" err=\"{}\"", webhook_name, error_msg),
                    format!("\"Failed to apply resource\" kind=\"{}\" name=\"{}\" err=\"{}\"", kind, name, error_msg),
                    format!("\"Failed to authenticate request\" err=\"invalid token\""),
                    format!("\"Too many requests\" user=\"{}\"", user),
                ];
                messages.choose(rng).unwrap_or(&"Default API Server Error".to_string()).to_string()
            }
            LogLevel::Fatal => format!("\"API server shutting down\" reason=\"{}\"", error_msg),
        },
        "kube-scheduler" => match level {
            LogLevel::Info => {
                let messages = [
                    format!("\"Attempting to schedule pod\" pod=\"{}\"", pod_obj.to_namespaced_name()),
                    format!("\"Successfully bound pod to node\" pod=\"{}\" node=\"{}\"", pod_obj.to_namespaced_name(), node_name),
                    format!("\"Starting Kubernetes scheduler\" version=\"v1.25.3\""),
                    format!("\"Found N preemption victims\" count={}", rng.gen_range(1..5)),
                ];
                messages.choose(rng).unwrap_or(&"Default Scheduler Info".to_string()).to_string()
            }
            LogLevel::Warning | LogLevel::Error => {
                 let messages = [
                    format!("\"Failed to schedule pod\" pod=\"{}\" err=\"insufficient {}\"", pod_obj.to_namespaced_name(), ["cpu", "memory", "gpu"].choose(rng).unwrap_or(&"resources")),
                    format!("\"No nodes available to schedule pod\" pod=\"{}\"", pod_obj.to_namespaced_name()),
                    format!("\"Failed to find fit for pod\" pod=\"{}\" err=\"node(s) didn't match node selector\"", pod_obj.to_namespaced_name()),
                ];
                messages.choose(rng).unwrap_or(&"Default Scheduler Error".to_string()).to_string()
            }
            LogLevel::Fatal => format!("\"Scheduler process exiting\" reason=\"{}\"", error_msg),
        },
        "etcd" => match level { // etcd logs are often simpler, more direct
            LogLevel::Info => {
                let from_id = format!("{:x}", rng.gen::<u64>());
                let to_id = format!("{:x}", rng.gen::<u64>());
                let proposal_id = format!("{:x}", rng.gen::<u64>());
                let messages = [
                    format!("\"leader changed\" from=\"{}\" to=\"{}\"", from_id, to_id),
                    format!("\"applied proposal\" id=\"{}\" size=\"{} bytes\"", proposal_id, rng.gen_range(100..5000)),
                    format!("\"starting etcd server\" version=\"3.5.4\" data-dir=\"/var/lib/etcd\""),
                    format!("\"publish attributes\" local_id={} term={}", from_id, rng.gen_range(1..10)),
                ];
                messages.choose(rng).unwrap_or(&"Default etcd Info".to_string()).to_string()
            }
            LogLevel::Warning | LogLevel::Error => {
                 let messages = [
                    format!("\"failed to reach quorum\" current_peers={}", rng.gen_range(1..3)),
                    format!("\"slow fdatasync\" duration=\"{}ms\"", rng.gen_range(100..2000)),
                    format!("\"apply request took too long\" duration=\"{}ms\"", rng.gen_range(500..3000)),
                    format!("\"connection refused from\" remote_peer_id=\"{:x}\"", rng.gen::<u64>()),
                ];
                messages.choose(rng).unwrap_or(&"Default etcd Error".to_string()).to_string()
            }
            LogLevel::Fatal => format!("\"etcd server critical error\" reason=\"{}\"", error_msg),
        },
        "kube-controller-manager" => match level {
            LogLevel::Info => {
                let messages = [
                    format!("\"Starting controller\" name=\"{}\"", controller_name),
                    format!("\"Processing item\" controller=\"{}\" key=\"{}\"", controller_name, item_key),
                    format!("\"Successfully processed item\" controller=\"{}\" key=\"{}\"", controller_name, item_key),
                    format!("\"Starting Kubernetes controller manager\" version=\"v1.25.3\""),
                ];
                messages.choose(rng).unwrap_or(&"Default ControllerManager Info".to_string()).to_string()
            }
            LogLevel::Warning | LogLevel::Error => {
                let messages = [
                    format!("\"Error syncing controller\" controller=\"{}\" key=\"{}\" err=\"{}\"", controller_name, item_key, error_msg),
                    format!("\"Failed to update status for resource\" kind=\"{}\" name=\"{}\" err=\"{}\"", K8S_KINDS.choose(rng).unwrap_or(&"Deployment"), item_key, error_msg),
                    format!("\"Retrying sync\" controller=\"{}\" key=\"{}\"", controller_name, item_key),
                ];
                messages.choose(rng).unwrap_or(&"Default ControllerManager Error".to_string()).to_string()
            }
            LogLevel::Fatal => format!("\"Controller manager exiting\" reason=\"{}\"", error_msg),
        },
        "kube-proxy" => match level {
            LogLevel::Info => {
                let messages = [
                    format!("\"Syncing iptables rules\" rule_count={}", rng.gen_range(100..5000)),
                    format!("\"Successfully synced service\" service=\"{}\"", item_key),
                    format!("\"Starting Kubernetes kube-proxy\" version=\"v1.25.3\""),
                    format!("\"Detected new service\" service=\"{}\"", item_key),
                ];
                messages.choose(rng).unwrap_or(&"Default KubeProxy Info".to_string()).to_string()
            }
            LogLevel::Warning | LogLevel::Error => {
                 let messages = [
                    format!("\"Error syncing service\" service=\"{}\" err=\"{}\"", item_key, error_msg),
                    format!("\"Failed to connect to apiserver\" err=\"{}\"", error_msg),
                    format!("\"IPTables chain already exists\" chain=\"KUBE-SERVICES\""),
                ];
                messages.choose(rng).unwrap_or(&"Default KubeProxy Error".to_string()).to_string()
            }
            LogLevel::Fatal => format!("\"Kube-proxy critical failure\" reason=\"{}\"", error_msg),
        },
        _ => format!("\"Default message for unknown component\" component=\"{}\" level=\"{}\"", component, level),
    }
}


pub fn generate_k8s_infra_logs(count: usize, seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut logs = Vec::with_capacity(count);
    let mut current_time = Utc::now() - Duration::days(rng.gen_range(1..3)); // Start up to 3 days ago

    let components_files: Vec<(&str, &[&str])> = vec![
        ("kubelet", KUBELET_FILES),
        ("kube-apiserver", APISERVER_FILES),
        ("kube-scheduler", SCHEDULER_FILES),
        ("etcd", ETCD_FILES),
        ("kube-controller-manager", CONTROLLER_MANAGER_FILES),
        ("kube-proxy", KUBE_PROXY_FILES),
    ];

    // Weighted log levels: I: 65%, W: 20%, E: 10%, F: 5%
    let log_level_choices = [
        (LogLevel::Info, 65), 
        (LogLevel::Warning, 20), 
        (LogLevel::Error, 10), 
        (LogLevel::Fatal, 5)
    ];
    let levels: Vec<LogLevel> = log_level_choices
        .iter()
        .flat_map(|&(val, weight)| std::iter::repeat(val).take(weight))
        .collect();

    for _i in 0..count {
        current_time += Duration::milliseconds(rng.gen_range(10..2000)); // Increment time

        let (component_name, file_options) = components_files.choose(&mut rng).unwrap();
        let file_line = file_options.choose(&mut rng).unwrap_or(&"unknown.go:0").to_string();
        let log_level = *levels.choose(&mut rng).unwrap_or(&LogLevel::Info);
        
        let message = generate_message_for_component(component_name, log_level, &mut rng);
        let thread_id = if rng.gen_bool(0.9) { 1 } else { rng.gen_range(2..20) }; // Most logs from thread 1

        logs.push(format_klog_entry(
            log_level,
            &current_time,
            thread_id,
            &file_line,
            &message,
        ));
    }

    logs
}
