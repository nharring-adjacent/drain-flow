// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

//! Generates realistic klog-formatted logs for Kubernetes infrastructure components (e.g., Kubelet, API server).

use chrono::{DateTime, Duration, Utc};
use rand::prelude::IndexedRandom;
use rand::{rngs::StdRng, Rng, SeedableRng}; // SliceRandom removed as IndexedRandom should cover .choose() on slices // For .choose()
                                            // Removed: use rand::seq::SliceRandom;
use std::fmt::{Display, Formatter, Result as FmtResult};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
enum LogLevel {
    Info,
    Warning,
    Error,
    Fatal,
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            LogLevel::Info => write!(f, "I"),
            LogLevel::Warning => write!(f, "W"),
            LogLevel::Error => write!(f, "E"),
            LogLevel::Fatal => write!(f, "F"),
        }
    }
}

#[derive(Debug)]
struct K8sObjectRef {
    kind: String,
    namespace: String,
    name: String,
    uid: String,
}

impl K8sObjectRef {
    fn new(
        kind: Option<&str>,
        namespace: Option<&str>,
        name_prefix: Option<&str>,
        rng: &mut StdRng,
    ) -> Self {
        let actual_kind = kind
            .unwrap_or_else(|| K8S_KINDS.choose(rng).unwrap_or(&"Pod"))
            .to_string();
        let actual_ns = namespace
            .unwrap_or_else(|| NAMESPACES.choose(rng).unwrap_or(&"default"))
            .to_string();
        let actual_name = format!(
            "{}{}",
            name_prefix.unwrap_or_else(|| POD_NAMES.choose(rng).unwrap_or(&"app")),
            rng.random_range(10000..99999)
        );
        Self {
            kind: actual_kind,
            namespace: actual_ns,
            name: actual_name,
            uid: Uuid::new_v4().to_string(), // Does not require rng passed in
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

// Constants for generating varied log messages
const K8S_KINDS: &[&str] = &[
    "Pod",
    "Node",
    "Service",
    "Deployment",
    "ReplicaSet",
    "PersistentVolumeClaim",
    "Endpoints",
    "ConfigMap",
    "Secret",
];
const NAMESPACES: &[&str] = &[
    "default",
    "kube-system",
    "production",
    "staging",
    "dev-team-a",
    "monitoring",
    "logging",
];
const POD_NAMES: &[&str] = &[
    "frontend-",
    "backend-",
    "worker-",
    "database-",
    "cache-",
    "job-runner-",
];
const NODE_NAMES: &[&str] = &[
    "node-prod-01",
    "node-worker-b7",
    "kind-control-plane",
    "dev-node-spot-instance",
];
const CONTAINER_IMAGE_NAMES: &[&str] = &[
    "nginx:1.21-alpine",
    "postgres:14.2",
    "custom-app:v2.3.1",
    "redis:6.0",
    "pause:3.5",
];
const ERROR_MESSAGES: &[&str] = &[
    "operation cannot be fulfilled",
    "an error on the server",
    "forbidden",
    "not found",
    "timeout",
    "conflict",
    "insufficient resources",
    "no nodes available",
    "failed to pull image",
    "pod sync failed",
    "failed to update node status",
    "admission webhook denied request",
    "failed to apply resource",
    "leader election lost",
    "slow fdatasync",
    "failed to reach quorum",
    "error syncing X",
    "failed to ensure load balancer",
];
const USERS: &[&str] = &[
    "system:kube-scheduler",
    "system:serviceaccount:kube-system:deployment-controller",
    "user-dev@example.com",
    "system:node:node-worker-b7",
];
const GROUPS: &[&str] = &[
    "system:nodes",
    "system:authenticated",
    "system:masters",
    "developers",
];
const WEBHOOK_NAMES: &[&str] = &[
    "validation.example.com",
    "mutation.internal.svc",
    "kyverno-policy-controller",
];
const CONTROLLER_NAMES: &[&str] = &[
    "DeploymentController",
    "ReplicaSetController",
    "NodeController",
    "ServiceAccountController",
    "EndpointSliceController",
];
const KUBELET_FILES: &[&str] = &[
    "kubelet.go",
    "pleg.go",
    "volume_manager.go",
    "prober.go",
    "server.go",
    "container_manager_linux.go",
    "oom_watcher_linux.go",
];
const APISERVER_FILES: &[&str] = &[
    "storage_rbac.go",
    "generic_apiserver.go",
    "authentication.go",
    "audit.go",
    "etcd_health.go",
];
const SCHEDULER_FILES: &[&str] = &[
    "scheduler.go",
    "core_scheduler.go",
    "priorities.go",
    "binder.go",
    "framework.go",
];
const ETCD_FILES: &[&str] = &["server.go", "raft.go", "mvcc_db.go", "wal.go", "cluster.go"];
const CONTROLLERMANAGER_FILES: &[&str] = &[
    "node_controller.go",
    "deployment_controller.go",
    "serviceaccount_controller.go",
    "endpoints_controller.go",
    "job_controller.go",
];
const KUBEPROXY_FILES: &[&str] = &["proxier.go", "service.go", "endpoints.go", "iptables.go"];

fn generate_k8s_object(
    rng: &mut StdRng,
    kind: Option<&str>,
    namespace: Option<&str>,
    name_prefix: Option<&str>,
) -> K8sObjectRef {
    K8sObjectRef::new(kind, namespace, name_prefix, rng)
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
fn generate_message_for_component(
    component_name: &str,
    level: LogLevel,
    rng: &mut StdRng,
) -> String {
    let pod_obj = generate_k8s_object(rng, Some("Pod"), None, None);
    let node_name = NODE_NAMES.choose(rng).unwrap_or(&"node-unknown");
    let image_name = CONTAINER_IMAGE_NAMES
        .choose(rng)
        .unwrap_or(&"unknown-image");
    let error_msg_snippet = ERROR_MESSAGES.choose(rng).unwrap_or(&"unknown error");
    let user = USERS.choose(rng).unwrap_or(&"system:unknown");
    let groups_vec: Vec<&&str> = GROUPS.iter().filter(|_| rng.random_bool(0.3)).collect(); // Select some groups
    let groups_str = groups_vec
        .iter()
        .map(|s| **s)
        .collect::<Vec<&str>>()
        .join(",");
    let webhook_name = WEBHOOK_NAMES.choose(rng).unwrap_or(&"unknown-webhook");
    let controller_name = CONTROLLER_NAMES.choose(rng).unwrap_or(&"UnknownController");

    match component_name {
        "kubelet" => match level {
            LogLevel::Info => {
                let messages = [
                    format!("Event({}): type: 'Normal' reason: 'Scheduled' Successfully assigned {} to {}", pod_obj.to_log_string(), pod_obj.to_namespaced_name(), node_name),
                    format!("SyncLoop (ADD, \"api\"): \"{}_{}_{}\"", pod_obj.uid, pod_obj.namespace, pod_obj.name),
                    format!("\"Adding pod to network\" pod=\"{}\"", pod_obj.to_namespaced_name()),
                    format!("\"Updating status for pod\" pod=\"{}\" status={{phase:\"{}\"}}", pod_obj.to_namespaced_name(), ["Running", "Succeeded"].choose(rng).unwrap_or(&"Running")),
                    format!("\"Image GC completed\" images_deleted={} bytes_reclaimed={}", rng.random_range(0..5), rng.random_range(0..1024*1024*500)),
                    format!("\"Starting PLEG\""),
                    format!("\"Podsandbox changed\" pod=\"{}\" sandbox=\"{}\"", pod_obj.to_namespaced_name(), Uuid::new_v4().to_string().chars().take(12).collect::<String>()),
                ];
                messages
                    .choose(rng)
                    .unwrap_or(&"Default Kubelet Info".to_string())
                    .to_string()
            }
            LogLevel::Warning | LogLevel::Error | LogLevel::Fatal => {
                let messages = [
                    format!("\"Failed to pull image\" image=\"{}\" err=\"ImagePullBackOff: {}\"", image_name, error_msg_snippet),
                    format!("\"Pod sync failed\" pod=\"{}\" err=\"{}\"", pod_obj.to_namespaced_name(), error_msg_snippet),
                    format!("\"Failed to update node status\" err=\"timeout attempting to reach API server: {}\"", error_msg_snippet),
                    format!("\"Container runtime network not ready\" networkReady=\"false\" message=\"{}\"", error_msg_snippet),
                    format!("\"Eviction manager: attempting to reclaim\" resourceName=\"memory\""),
                    format!("\"PLEG is not healthy: pleg was last seen active {}, but is now {}", Duration::seconds(rng.random_range(60..300)).num_seconds(), Duration::seconds(rng.random_range(5..59)).num_seconds()),
                ];
                messages
                    .choose(rng)
                    .unwrap_or(&"Default Kubelet Error".to_string())
                    .to_string()
            }
        },
        "kube-apiserver" => match level {
            LogLevel::Info => {
                let method = ["GET", "POST", "PUT", "DELETE"]
                    .choose(rng)
                    .unwrap_or(&"GET");
                let path_kind = K8S_KINDS.choose(rng).unwrap_or(&"pods");
                let event = ["ADDED", "MODIFIED", "DELETED"]
                    .choose(rng)
                    .unwrap_or(&"ADDED");
                let messages = [
                    format!("\"Handling request\" method=\"{}\" path=\"/api/v1/namespaces/{}/{}\"", method, pod_obj.namespace, path_kind),
                    format!("\"Authentication attempt\" user=\"{}\" groups=\"[{}]\"", user, groups_str),
                    format!("\"Resource event\" kind=\"{}\" event=\"{}\" name=\"{}\"", K8S_KINDS.choose(rng).unwrap_or(&"Pod"), event, pod_obj.name),
                    format!("\"Audit event\" id=\"{}\" stage=\"ResponseComplete\" user=\"{}\" verb=\"get\" resource=\"{}\" namespace=\"{}\" name=\"{}\" responseStatus=\"200 OK\"", Uuid::new_v4(), user, path_kind, pod_obj.namespace, pod_obj.name),
                ];
                messages
                    .choose(rng)
                    .unwrap_or(&"Default API Server Info".to_string())
                    .to_string()
            }
            LogLevel::Warning | LogLevel::Error | LogLevel::Fatal => {
                let kind = K8S_KINDS.choose(rng).unwrap_or(&"Deployment");
                let messages = [
                    format!("\"Admission webhook denied request\" webhook=\"{}\" kind=\"{}\" name=\"{}\" err=\"{}\"", webhook_name, kind, pod_obj.name, error_msg_snippet),
                    format!("\"Failed to apply resource\" kind=\"{}\" name=\"{}\" err=\"{}\"", kind, pod_obj.name, error_msg_snippet),
                    format!("\"Rate limit exceeded\" user=\"{}\" sourceIP=\"10.1.2.3\"", user),
                    format!("\"Etcd health check failed\" err=\"{}\"", error_msg_snippet),
                ];
                messages
                    .choose(rng)
                    .unwrap_or(&"Default API Server Error".to_string())
                    .to_string()
            }
        },
        "kube-scheduler" => {
            match level {
                LogLevel::Info => {
                    let messages = [
                    format!("\"Attempting to schedule pod\" pod=\"{}\"", pod_obj.to_namespaced_name()),
                    format!("\"Successfully bound pod to node\" pod=\"{}\" node=\"{}\"", pod_obj.to_namespaced_name(), node_name),
                    format!("\"Predicate failed\" predicate=\"NodeAffinity\" pod=\"{}\" node=\"{}\"", pod_obj.to_namespaced_name(), node_name),
                    format!("\"Found N preemption victims\" count={}", rng.random_range(1..5)),
                ];
                    messages
                        .choose(rng)
                        .unwrap_or(&"Default Scheduler Info".to_string())
                        .to_string()
                }
                LogLevel::Warning | LogLevel::Error | LogLevel::Fatal => {
                    let messages = [
                        format!(
                            "\"Failed to schedule pod\" pod=\"{}\" err=\"insufficient {}\"",
                            pod_obj.to_namespaced_name(),
                            ["cpu", "memory", "gpu"].choose(rng).unwrap_or(&"resources")
                        ),
                        format!(
                            "\"No nodes available to schedule pod\" pod=\"{}\"",
                            pod_obj.to_namespaced_name()
                        ),
                        format!(
                            "\"Error binding pod to node\" pod=\"{}\" node=\"{}\" err=\"{}\"",
                            pod_obj.to_namespaced_name(),
                            node_name,
                            error_msg_snippet
                        ),
                    ];
                    messages
                        .choose(rng)
                        .unwrap_or(&"Default Scheduler Error".to_string())
                        .to_string()
                }
            }
        }
        "etcd" => match level {
            LogLevel::Info => {
                let from_id = format!("{:x}", rng.random::<u64>());
                let to_id = format!("{:x}", rng.random::<u64>());
                let proposal_id = format!("{:x}", rng.random::<u64>());
                let messages = [
                    format!("\"leader changed\" from=\"{}\" to=\"{}\"", from_id, to_id),
                    format!(
                        "\"applied proposal\" id=\"{}\" size=\"{} bytes\"",
                        proposal_id,
                        rng.random_range(100..5000)
                    ),
                    format!(
                        "\"raft.node: {} elected leader {} at term {}\"",
                        from_id,
                        to_id,
                        rng.random_range(10..100)
                    ),
                    format!(
                        "\"publish attributes\" local_id={} term={}",
                        from_id,
                        rng.random_range(1..10)
                    ),
                ];
                messages
                    .choose(rng)
                    .unwrap_or(&"Default etcd Info".to_string())
                    .to_string()
            }
            LogLevel::Warning | LogLevel::Error | LogLevel::Fatal => {
                let messages = [
                    format!(
                        "\"failed to reach quorum\" current_peers={}",
                        rng.random_range(1..3)
                    ),
                    format!(
                        "\"slow fdatasync\" duration=\"{}ms\"",
                        rng.random_range(100..2000)
                    ),
                    format!(
                        "\"apply request took too long\" duration=\"{}ms\"",
                        rng.random_range(500..3000)
                    ),
                    format!(
                        "\"connection refused from\" remote_peer_id=\"{:x}\"",
                        rng.random::<u64>()
                    ),
                ];
                messages
                    .choose(rng)
                    .unwrap_or(&"Default etcd Error".to_string())
                    .to_string()
            }
        },
        "kube-controller-manager" => {
            let item_key = format!("{}/{}", pod_obj.namespace, pod_obj.name);
            match level {
                LogLevel::Info => {
                    let messages = [
                        format!("\"Starting controller\" name=\"{}\"", controller_name),
                        format!("\"Processing item\" controller=\"{}\" key=\"{}\"", controller_name, item_key),
                        format!("\"Successfully processed item\" controller=\"{}\" key=\"{}\"", controller_name, item_key),
                        format!("\"Finished processing item\" controller=\"{}\" key=\"{}\" duration=\"{}ms\"", controller_name, item_key, rng.random_range(10..500)),
                    ];
                    messages
                        .choose(rng)
                        .unwrap_or(&"Default ControllerManager Info".to_string())
                        .to_string()
                }
                LogLevel::Warning | LogLevel::Error | LogLevel::Fatal => {
                    let messages = [
                        format!("\"Error syncing resource\" controller=\"{}\" key=\"{}\" err=\"{}\"", controller_name, item_key, error_msg_snippet),
                        format!("\"Failed to update status for resource\" kind=\"{}\" name=\"{}\" err=\"{}\"", K8S_KINDS.choose(rng).unwrap_or(&"Deployment"), item_key, error_msg_snippet),
                        format!("\"Requeuing item due to error\" controller=\"{}\" key=\"{}\" err=\"{}\"", controller_name, item_key, error_msg_snippet),
                    ];
                    messages
                        .choose(rng)
                        .unwrap_or(&"Default ControllerManager Error".to_string())
                        .to_string()
                }
            }
        }
        "kube-proxy" => match level {
            LogLevel::Info => {
                let messages = [
                    format!(
                        "\"Syncing iptables rules\" rule_count={}",
                        rng.random_range(100..5000)
                    ),
                    format!("\"Successfully synced iptables rules\""),
                    format!(
                        "\"Adding new service\" service=\"{}/{}\" ip=\"10.0.1.2\" port=\"80\"",
                        pod_obj.namespace, pod_obj.name
                    ),
                    format!(
                        "\"Updating service\" service=\"{}/{}\"",
                        pod_obj.namespace, pod_obj.name
                    ),
                ];
                messages
                    .choose(rng)
                    .unwrap_or(&"Default KubeProxy Info".to_string())
                    .to_string()
            }
            LogLevel::Warning | LogLevel::Error | LogLevel::Fatal => {
                let messages = [
                    format!(
                        "\"Error syncing iptables rules\" err=\"{}\"",
                        error_msg_snippet
                    ),
                    format!(
                        "\"Failed to update service\" service=\"{}/{}\" err=\"{}\"",
                        pod_obj.namespace, pod_obj.name, error_msg_snippet
                    ),
                    format!(
                        "\"Endpoint not found for service\" service=\"{}/{}\"",
                        pod_obj.namespace, pod_obj.name
                    ),
                ];
                messages
                    .choose(rng)
                    .unwrap_or(&"Default KubeProxy Error".to_string())
                    .to_string()
            }
        },
        _ => format!(
            "Generic message for {} - ID: {}",
            component_name,
            rng.random::<u32>()
        ),
    }
}

pub fn format_klog_entry(
    level: LogLevel,
    timestamp: &chrono::DateTime<Utc>,
    thread_id: u32,
    file_line: &str,
    message: &str,
) -> String {
    format!(
        "{} {} {:>7} {}:{}] {}",
        level,
        timestamp.format("%m%d %H:%M:%S.%f"), // klog timestamp format
        thread_id,
        file_line.split(':').next().unwrap_or("unknownfile.go"), // filename part
        file_line.split(':').nth(1).unwrap_or("0"),              // line number part
        message
    )
}

pub fn generate_k8s_infra_logs(count: usize, seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut logs = Vec::with_capacity(count);
    let mut current_time = Utc::now() - Duration::days(rng.random_range(1..7));

    let components_files: Vec<(&str, &[&str])> = vec![
        ("kubelet", &KUBELET_FILES),
        ("kube-apiserver", &APISERVER_FILES),
        ("kube-scheduler", &SCHEDULER_FILES),
        ("etcd", &ETCD_FILES),
        ("kube-controller-manager", &CONTROLLERMANAGER_FILES),
        ("kube-proxy", &KUBEPROXY_FILES),
    ];

    for _ in 0..count {
        current_time += Duration::milliseconds(rng.random_range(10..2000));
        let (component_name, file_options) = components_files.choose(&mut rng).unwrap();
        let file_name_part = file_options.choose(&mut rng).unwrap_or(&"default.go");
        let line_num = rng.random_range(50..1500);
        let file_line = format!("{}:{}", file_name_part, line_num);

        let levels = [
            LogLevel::Info,
            LogLevel::Warning,
            LogLevel::Error,
            LogLevel::Fatal,
        ];
        let log_level = *levels.choose(&mut rng).unwrap_or(&LogLevel::Info);

        let message = generate_message_for_component(component_name, log_level, &mut rng);
        let thread_id = rng.random_range(1..1000); // Can vary a bit more

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
