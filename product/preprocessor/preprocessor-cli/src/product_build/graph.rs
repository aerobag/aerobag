use super::*;

#[derive(Debug, Clone)]
pub(super) struct GraphScheduledTask<K> {
    pub(super) id: String,
    pub(super) deps: Vec<String>,
    pub(super) weight: usize,
    pub(super) kind: K,
}

#[derive(Debug, Clone)]
pub(super) struct GraphTaskCompletion<V> {
    pub(super) node_records: Vec<NodeRecord>,
    pub(super) value: V,
    pub(super) completion_detail: String,
}

struct GraphCompletionGuard<V> {
    tx: crossbeam_channel::Sender<(String, usize, anyhow::Result<GraphTaskCompletion<V>>)>,
    task_id: String,
    task_weight: usize,
    sent: bool,
}

impl<V> GraphCompletionGuard<V> {
    fn new(
        tx: crossbeam_channel::Sender<(String, usize, anyhow::Result<GraphTaskCompletion<V>>)>,
        task_id: String,
        task_weight: usize,
    ) -> Self {
        Self {
            tx,
            task_id,
            task_weight,
            sent: false,
        }
    }

    fn send(mut self, result: anyhow::Result<GraphTaskCompletion<V>>) {
        let _ = self
            .tx
            .send((self.task_id.clone(), self.task_weight, result));
        self.sent = true;
    }
}

impl<V> Drop for GraphCompletionGuard<V> {
    fn drop(&mut self) {
        if self.sent {
            return;
        }

        let _ = self.tx.send((
            self.task_id.clone(),
            self.task_weight,
            Err(anyhow::anyhow!(
                "graph task worker exited without delivering completion"
            )),
        ));
    }
}

pub(super) fn run_weighted_task_graph<K, V, RunTask, Log>(
    graph_name: &str,
    mut pending_tasks: Vec<GraphScheduledTask<K>>,
    work_unit_budget: usize,
    mut log: Log,
    run_task: RunTask,
) -> anyhow::Result<(BTreeMap<String, V>, BTreeMap<String, Vec<NodeRecord>>)>
where
    K: Clone + Send + 'static,
    V: Clone + Send + 'static,
    RunTask: Fn(
            K,
            BTreeMap<String, V>,
            BTreeMap<String, Vec<NodeRecord>>,
        ) -> anyhow::Result<GraphTaskCompletion<V>>
        + Clone
        + Send
        + 'static,
    Log: FnMut(String) -> anyhow::Result<()>,
{
    let total_tasks = pending_tasks.len();
    log(format!(
        "{graph_name}-ready tasks={} work_unit_budget={}",
        total_tasks, work_unit_budget
    ))?;

    let (tx, rx) =
        crossbeam_channel::unbounded::<(String, usize, anyhow::Result<GraphTaskCompletion<V>>)>();
    let mut running_jobs = 0_usize;
    let mut running_units = 0_usize;
    let mut launched_tasks = 0_usize;
    let mut completed_tasks = 0_usize;
    let mut completed_ids = std::collections::BTreeSet::<String>::new();
    let mut task_values = BTreeMap::<String, V>::new();
    let mut task_node_records = BTreeMap::<String, Vec<NodeRecord>>::new();
    let mut worker_threads = BTreeMap::<String, thread::JoinHandle<anyhow::Result<()>>>::new();

    while running_jobs > 0 || !pending_tasks.is_empty() {
        let mut launched_any = false;
        let mut index = 0_usize;
        while index < pending_tasks.len() {
            let task = &pending_tasks[index];
            let deps_ready = task.deps.iter().all(|dep| completed_ids.contains(dep));
            let fits_budget = running_units + task.weight <= work_unit_budget;
            if !deps_ready || !fits_budget {
                index += 1;
                continue;
            }

            let task = pending_tasks.remove(index);
            let task_id = task.id.clone();
            let task_weight = task.weight;
            launched_tasks += 1;
            log(format!(
                "{graph_name}-launch {} launched={}/{} completed={}/{} weight={} running_units={}/{}",
                task_id,
                launched_tasks,
                total_tasks,
                completed_tasks,
                total_tasks,
                task_weight,
                running_units + task_weight,
                work_unit_budget,
            ))?;
            let tx = tx.clone();
            let task_values_snapshot = task_values.clone();
            let task_node_records_snapshot = task_node_records.clone();
            let worker_task_id = task_id.clone();
            let run_task = run_task.clone();
            let join_handle = thread::spawn(move || -> anyhow::Result<()> {
                let task_label = worker_task_id.clone();
                let completion_guard =
                    GraphCompletionGuard::new(tx, worker_task_id.clone(), task_weight);
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    run_task(task.kind, task_values_snapshot, task_node_records_snapshot)
                }))
                .unwrap_or_else(|panic_payload| {
                    let panic_text = if let Some(text) = panic_payload.downcast_ref::<&str>() {
                        (*text).to_string()
                    } else if let Some(text) = panic_payload.downcast_ref::<String>() {
                        text.clone()
                    } else {
                        "unknown panic payload".to_string()
                    };
                    Err(anyhow::anyhow!(
                        "graph task thread panicked: {task_label}: {panic_text}"
                    ))
                });
                completion_guard.send(result);
                Ok(())
            });
            worker_threads.insert(task_id.clone(), join_handle);
            running_jobs += 1;
            running_units += task_weight;
            launched_any = true;
        }

        if running_jobs == 0 {
            if pending_tasks.is_empty() {
                break;
            }
            bail!("{graph_name} deadlock: no runnable tasks remain");
        }
        if !launched_any {
            // wait for a running task to free capacity or satisfy dependencies
        }

        let (task_id, task_weight, result) = loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(message) => break message,
                Err(RecvTimeoutError::Timeout) => {
                    log(format!(
                        "{graph_name}-wait running_jobs={} pending_tasks={} running_units={}/{}",
                        running_jobs,
                        pending_tasks.len(),
                        running_units,
                        work_unit_budget,
                    ))?;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("{graph_name} channel closed unexpectedly");
                }
            }
        };
        running_jobs -= 1;
        running_units = running_units.saturating_sub(task_weight);
        if let Some(handle) = worker_threads.remove(&task_id) {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("failed to join graph worker {task_id}"))??;
        }
        match result {
            Ok(completion) => {
                completed_tasks += 1;
                task_node_records.insert(task_id.clone(), completion.node_records.clone());
                completed_ids.insert(task_id.clone());
                task_values.insert(task_id.clone(), completion.value);
                log(format!(
                    "{graph_name}-complete {} completed={}/{} running_units={}/{} {}",
                    task_id,
                    completed_tasks,
                    total_tasks,
                    running_units,
                    work_unit_budget,
                    completion.completion_detail,
                ))?;
            }
            Err(err) => {
                log(format!("{graph_name}-complete {task_id} FAIL error={err}"))?;
                return Err(err);
            }
        }
    }

    Ok((task_values, task_node_records))
}
