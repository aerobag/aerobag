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

#[derive(Debug, Clone)]
pub(super) struct GraphRunOutcome<V> {
    pub(super) task_values: BTreeMap<String, V>,
    pub(super) task_node_records: BTreeMap<String, Vec<NodeRecord>>,
    pub(super) failures: BTreeMap<String, String>,
    pub(super) skipped_tasks: BTreeSet<String>,
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

#[allow(dead_code)]
pub(super) fn run_weighted_task_graph<K, V, RunTask, Log>(
    graph_name: &str,
    pending_tasks: Vec<GraphScheduledTask<K>>,
    work_unit_budget: usize,
    log: Log,
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
    let outcome = run_weighted_task_graph_impl(
        graph_name,
        pending_tasks,
        work_unit_budget,
        log,
        run_task,
        |_, _, _, _, _| Ok(Vec::new()),
        GraphFailurePolicy::FailFast,
    )?;
    Ok((outcome.task_values, outcome.task_node_records))
}

pub(super) fn run_weighted_task_graph_with_expansion<K, V, RunTask, ExpandTask, Log>(
    graph_name: &str,
    pending_tasks: Vec<GraphScheduledTask<K>>,
    work_unit_budget: usize,
    log: Log,
    run_task: RunTask,
    expand_task: ExpandTask,
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
    ExpandTask: FnMut(
        &str,
        &K,
        &GraphTaskCompletion<V>,
        &BTreeMap<String, V>,
        &BTreeMap<String, Vec<NodeRecord>>,
    ) -> anyhow::Result<Vec<GraphScheduledTask<K>>>,
    Log: FnMut(String) -> anyhow::Result<()>,
{
    let outcome = run_weighted_task_graph_impl(
        graph_name,
        pending_tasks,
        work_unit_budget,
        log,
        run_task,
        expand_task,
        GraphFailurePolicy::FailFast,
    )?;
    Ok((outcome.task_values, outcome.task_node_records))
}

#[allow(dead_code)]
pub(super) fn run_weighted_task_graph_fail_slow_with_failure_scopes<
    K,
    V,
    FailureScope,
    RunTask,
    Log,
>(
    graph_name: &str,
    pending_tasks: Vec<GraphScheduledTask<K>>,
    work_unit_budget: usize,
    failure_scope: FailureScope,
    log: Log,
    run_task: RunTask,
) -> anyhow::Result<GraphRunOutcome<V>>
where
    K: Clone + Send + 'static,
    V: Clone + Send + 'static,
    FailureScope: Fn(&str) -> Option<String> + Clone + Send + 'static,
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
    run_weighted_task_graph_impl(
        graph_name,
        pending_tasks,
        work_unit_budget,
        log,
        run_task,
        |_, _, _, _, _| Ok(Vec::new()),
        GraphFailurePolicy::FailSlow(Box::new(failure_scope)),
    )
}

pub(super) fn run_weighted_task_graph_fail_slow_with_failure_scopes_and_expansion<
    K,
    V,
    FailureScope,
    RunTask,
    ExpandTask,
    Log,
>(
    graph_name: &str,
    pending_tasks: Vec<GraphScheduledTask<K>>,
    work_unit_budget: usize,
    failure_scope: FailureScope,
    log: Log,
    run_task: RunTask,
    expand_task: ExpandTask,
) -> anyhow::Result<GraphRunOutcome<V>>
where
    K: Clone + Send + 'static,
    V: Clone + Send + 'static,
    FailureScope: Fn(&str) -> Option<String> + Clone + Send + 'static,
    RunTask: Fn(
            K,
            BTreeMap<String, V>,
            BTreeMap<String, Vec<NodeRecord>>,
        ) -> anyhow::Result<GraphTaskCompletion<V>>
        + Clone
        + Send
        + 'static,
    ExpandTask: FnMut(
        &str,
        &K,
        &GraphTaskCompletion<V>,
        &BTreeMap<String, V>,
        &BTreeMap<String, Vec<NodeRecord>>,
    ) -> anyhow::Result<Vec<GraphScheduledTask<K>>>,
    Log: FnMut(String) -> anyhow::Result<()>,
{
    run_weighted_task_graph_impl(
        graph_name,
        pending_tasks,
        work_unit_budget,
        log,
        run_task,
        expand_task,
        GraphFailurePolicy::FailSlow(Box::new(failure_scope)),
    )
}

enum GraphFailurePolicy {
    FailFast,
    FailSlow(Box<dyn Fn(&str) -> Option<String> + Send>),
}

fn run_weighted_task_graph_impl<K, V, RunTask, ExpandTask, Log>(
    graph_name: &str,
    mut pending_tasks: Vec<GraphScheduledTask<K>>,
    work_unit_budget: usize,
    mut log: Log,
    run_task: RunTask,
    mut expand_task: ExpandTask,
    failure_policy: GraphFailurePolicy,
) -> anyhow::Result<GraphRunOutcome<V>>
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
    ExpandTask: FnMut(
        &str,
        &K,
        &GraphTaskCompletion<V>,
        &BTreeMap<String, V>,
        &BTreeMap<String, Vec<NodeRecord>>,
    ) -> anyhow::Result<Vec<GraphScheduledTask<K>>>,
    Log: FnMut(String) -> anyhow::Result<()>,
{
    let mut total_tasks = pending_tasks.len();
    log(format!(
        "{graph_name}-ready tasks={} work_unit_budget={}",
        total_tasks, work_unit_budget
    ))?;

    let mut known_task_ids = pending_tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<BTreeSet<_>>();
    let (tx, rx) =
        crossbeam_channel::unbounded::<(String, usize, anyhow::Result<GraphTaskCompletion<V>>)>();
    let mut running_jobs = 0_usize;
    let mut running_units = 0_usize;
    let mut launched_tasks = 0_usize;
    let mut completed_tasks = 0_usize;
    let mut completed_ids = std::collections::BTreeSet::<String>::new();
    let mut task_values = BTreeMap::<String, V>::new();
    let mut task_node_records = BTreeMap::<String, Vec<NodeRecord>>::new();
    let mut failures = BTreeMap::<String, String>::new();
    let mut failed_ids = BTreeSet::<String>::new();
    let mut failed_scopes = BTreeSet::<String>::new();
    let mut skipped_tasks = BTreeSet::<String>::new();
    let mut worker_threads = BTreeMap::<String, thread::JoinHandle<anyhow::Result<()>>>::new();
    let mut running_task_kinds = BTreeMap::<String, K>::new();

    while running_jobs > 0 || !pending_tasks.is_empty() {
        if matches!(failure_policy, GraphFailurePolicy::FailSlow(_)) {
            let mut index = 0_usize;
            while index < pending_tasks.len() {
                let failed_dep = pending_tasks[index]
                    .deps
                    .iter()
                    .find(|dep| failed_ids.contains(*dep) || skipped_tasks.contains(*dep))
                    .cloned();
                let failed_scope = match &failure_policy {
                    GraphFailurePolicy::FailFast => None,
                    GraphFailurePolicy::FailSlow(scope_for_task) => {
                        scope_for_task(&pending_tasks[index].id)
                            .filter(|scope| failed_scopes.contains(scope))
                    }
                };
                if failed_dep.is_some() || failed_scope.is_some() {
                    let task = pending_tasks.remove(index);
                    let skip_reason = if let Some(failed_dep) = failed_dep {
                        format!("failed_dep={failed_dep}")
                    } else {
                        format!(
                            "failed_scope={}",
                            failed_scope.expect("failed scope should exist")
                        )
                    };
                    log(format!(
                        "{graph_name}-skip {} skipped={}/{} {}",
                        task.id,
                        skipped_tasks.len() + 1,
                        total_tasks,
                        skip_reason
                    ))?;
                    skipped_tasks.insert(task.id);
                } else {
                    index += 1;
                }
            }
        }

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
            running_task_kinds.insert(task_id.clone(), task.kind.clone());
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
        let task_kind = running_task_kinds
            .remove(&task_id)
            .with_context(|| format!("missing running task kind for {task_id}"))?;
        match result {
            Ok(completion) => {
                completed_tasks += 1;
                let mut next_task_node_records = task_node_records.clone();
                next_task_node_records.insert(task_id.clone(), completion.node_records.clone());
                let mut next_task_values = task_values.clone();
                next_task_values.insert(task_id.clone(), completion.value.clone());
                let spawned_tasks = expand_task(
                    &task_id,
                    &task_kind,
                    &completion,
                    &next_task_values,
                    &next_task_node_records,
                )?;
                if !spawned_tasks.is_empty() {
                    for task in &spawned_tasks {
                        if !known_task_ids.insert(task.id.clone()) {
                            bail!("{graph_name} duplicate dynamic task id {}", task.id);
                        }
                    }
                    total_tasks += spawned_tasks.len();
                    log(format!(
                        "{graph_name}-spawn {} count={} total_tasks={}",
                        task_id,
                        spawned_tasks.len(),
                        total_tasks
                    ))?;
                    pending_tasks.extend(spawned_tasks);
                }
                task_node_records = next_task_node_records;
                task_values = next_task_values;
                completed_ids.insert(task_id.clone());
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
                let error_text = log_error_chain(&err);
                log(format!(
                    "{graph_name}-complete {task_id} FAIL error={error_text}"
                ))?;
                if matches!(failure_policy, GraphFailurePolicy::FailFast) {
                    return Err(err);
                }
                if let GraphFailurePolicy::FailSlow(scope_for_task) = &failure_policy {
                    if let Some(scope) = scope_for_task(&task_id) {
                        failed_scopes.insert(scope);
                    }
                }
                failed_ids.insert(task_id.clone());
                failures.insert(task_id, error_text);
            }
        }
    }

    Ok(GraphRunOutcome {
        task_values,
        task_node_records,
        failures,
        skipped_tasks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    enum TestTask {
        Pass(&'static str),
        Fail,
        FailWithContext,
    }

    #[test]
    fn fail_slow_continues_independent_tasks_and_skips_dependents() {
        let tasks = vec![
            GraphScheduledTask {
                id: "a".to_string(),
                deps: vec![],
                weight: 1,
                kind: TestTask::Pass("a"),
            },
            GraphScheduledTask {
                id: "b".to_string(),
                deps: vec![],
                weight: 1,
                kind: TestTask::Fail,
            },
            GraphScheduledTask {
                id: "c".to_string(),
                deps: vec!["a".to_string()],
                weight: 1,
                kind: TestTask::Pass("c"),
            },
            GraphScheduledTask {
                id: "d".to_string(),
                deps: vec!["b".to_string()],
                weight: 1,
                kind: TestTask::Pass("d"),
            },
            GraphScheduledTask {
                id: "e".to_string(),
                deps: vec![],
                weight: 1,
                kind: TestTask::Pass("e"),
            },
        ];
        let outcome = run_weighted_task_graph_fail_slow_with_failure_scopes(
            "test",
            tasks,
            2,
            |_| None,
            |_| Ok(()),
            |kind, _, _| match kind {
                TestTask::Pass(value) => Ok(GraphTaskCompletion {
                    node_records: vec![],
                    value: value.to_string(),
                    completion_detail: "ok".to_string(),
                }),
                TestTask::Fail => anyhow::bail!("synthetic failure"),
                TestTask::FailWithContext => {
                    Err(anyhow::anyhow!("inner diagnostic").context("outer failure"))
                }
            },
        )
        .unwrap();

        assert_eq!(
            outcome.task_values.keys().cloned().collect::<Vec<_>>(),
            vec!["a", "c", "e"]
        );
        assert_eq!(
            outcome.failures.keys().cloned().collect::<Vec<_>>(),
            vec!["b"]
        );
        assert_eq!(
            outcome.skipped_tasks.iter().cloned().collect::<Vec<_>>(),
            vec!["d"]
        );
    }

    #[test]
    fn dynamic_expansion_adds_tasks_after_completion() {
        let tasks = vec![GraphScheduledTask {
            id: "a".to_string(),
            deps: vec![],
            weight: 1,
            kind: TestTask::Pass("a"),
        }];
        let (values, _records) = run_weighted_task_graph_with_expansion(
            "test",
            tasks,
            1,
            |_| Ok(()),
            |kind, _, _| match kind {
                TestTask::Pass(value) => Ok(GraphTaskCompletion {
                    node_records: vec![],
                    value: value.to_string(),
                    completion_detail: "ok".to_string(),
                }),
                TestTask::Fail => anyhow::bail!("synthetic failure"),
                TestTask::FailWithContext => {
                    Err(anyhow::anyhow!("inner diagnostic").context("outer failure"))
                }
            },
            |task_id, _kind, _completion, _values, _records| {
                if task_id == "a" {
                    Ok(vec![GraphScheduledTask {
                        id: "b".to_string(),
                        deps: vec!["a".to_string()],
                        weight: 1,
                        kind: TestTask::Pass("b"),
                    }])
                } else {
                    Ok(Vec::new())
                }
            },
        )
        .unwrap();

        assert_eq!(values.keys().cloned().collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn fail_slow_preserves_nested_failure_diagnostics() {
        let tasks = vec![GraphScheduledTask {
            id: "a".to_string(),
            deps: vec![],
            weight: 1,
            kind: TestTask::FailWithContext,
        }];
        let outcome = run_weighted_task_graph_fail_slow_with_failure_scopes(
            "test",
            tasks,
            1,
            |_| None,
            |_| Ok(()),
            |kind, _, _| match kind {
                TestTask::Pass(value) => Ok(GraphTaskCompletion {
                    node_records: vec![],
                    value: value.to_string(),
                    completion_detail: "ok".to_string(),
                }),
                TestTask::Fail => anyhow::bail!("synthetic failure"),
                TestTask::FailWithContext => {
                    Err(anyhow::anyhow!("inner diagnostic").context("outer failure"))
                }
            },
        )
        .unwrap();

        let failure = outcome.failures.get("a").expect("task should fail");
        assert!(
            failure.contains("outer failure"),
            "missing outer context: {failure}"
        );
        assert!(
            failure.contains("inner diagnostic"),
            "missing inner diagnostic: {failure}"
        );
    }

    #[test]
    fn fail_slow_failure_scopes_skip_pending_tasks_in_failed_scope() {
        let tasks = vec![
            GraphScheduledTask {
                id: "2606:a".to_string(),
                deps: vec![],
                weight: 1,
                kind: TestTask::Pass("2606:a"),
            },
            GraphScheduledTask {
                id: "2607:a".to_string(),
                deps: vec![],
                weight: 1,
                kind: TestTask::Fail,
            },
            GraphScheduledTask {
                id: "2607:b".to_string(),
                deps: vec![],
                weight: 1,
                kind: TestTask::Pass("2607:b"),
            },
            GraphScheduledTask {
                id: "static".to_string(),
                deps: vec![],
                weight: 1,
                kind: TestTask::Pass("static"),
            },
        ];
        let outcome = run_weighted_task_graph_fail_slow_with_failure_scopes(
            "test",
            tasks,
            1,
            |task_id| task_id.split_once(':').map(|(scope, _)| scope.to_string()),
            |_| Ok(()),
            |kind, _, _| match kind {
                TestTask::Pass(value) => Ok(GraphTaskCompletion {
                    node_records: vec![],
                    value: value.to_string(),
                    completion_detail: "ok".to_string(),
                }),
                TestTask::Fail => anyhow::bail!("synthetic failure"),
                TestTask::FailWithContext => {
                    Err(anyhow::anyhow!("inner diagnostic").context("outer failure"))
                }
            },
        )
        .unwrap();

        assert_eq!(
            outcome.task_values.keys().cloned().collect::<Vec<_>>(),
            vec!["2606:a", "static"]
        );
        assert_eq!(
            outcome.failures.keys().cloned().collect::<Vec<_>>(),
            vec!["2607:a"]
        );
        assert_eq!(
            outcome.skipped_tasks.iter().cloned().collect::<Vec<_>>(),
            vec!["2607:b"]
        );
    }
}
