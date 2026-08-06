// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

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

#[derive(Debug)]
pub(super) struct GraphReadMap<T> {
    inner: Arc<RwLock<BTreeMap<String, T>>>,
}

impl<T> Clone for GraphReadMap<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Clone> GraphReadMap<T> {
    fn new(inner: Arc<RwLock<BTreeMap<String, T>>>) -> Self {
        Self { inner }
    }

    pub(super) fn with<R>(&self, key: &str, map: impl FnOnce(Option<&T>) -> R) -> R {
        let guard = self.inner.read().expect("graph read map lock poisoned");
        map(guard.get(key))
    }

    pub(super) fn get(&self, key: &str) -> Option<T> {
        self.inner
            .read()
            .expect("graph read map lock poisoned")
            .get(key)
            .cloned()
    }

    pub(super) fn iter(&self) -> std::vec::IntoIter<(String, T)> {
        self.inner
            .read()
            .expect("graph read map lock poisoned")
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>()
            .into_iter()
    }
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
    V: Clone + Send + Sync + 'static,
    RunTask: Fn(
            K,
            GraphReadMap<V>,
            GraphReadMap<Vec<NodeRecord>>,
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
    V: Clone + Send + Sync + 'static,
    FailureScope: Fn(&str) -> Option<String> + Clone + Send + 'static,
    RunTask: Fn(
            K,
            GraphReadMap<V>,
            GraphReadMap<Vec<NodeRecord>>,
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
    V: Clone + Send + Sync + 'static,
    FailureScope: Fn(&str) -> Option<String> + Clone + Send + 'static,
    RunTask: Fn(
            K,
            GraphReadMap<V>,
            GraphReadMap<Vec<NodeRecord>>,
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

fn graph_failed_scope(policy: &GraphFailurePolicy, task_id: &str) -> Option<String> {
    match policy {
        GraphFailurePolicy::FailFast => None,
        GraphFailurePolicy::FailSlow(scope_for_task) => scope_for_task(task_id),
    }
}

#[allow(clippy::too_many_arguments)]
fn skip_pending_graph_task<K, Log>(
    graph_name: &str,
    task_id: &str,
    skip_reason: String,
    total_tasks: usize,
    pending_tasks: &mut BTreeMap<String, GraphScheduledTask<K>>,
    unresolved_deps: &mut BTreeMap<String, usize>,
    reverse_deps: &mut BTreeMap<String, Vec<String>>,
    skipped_tasks: &mut BTreeSet<String>,
    log: &mut Log,
) -> anyhow::Result<()>
where
    Log: FnMut(String) -> anyhow::Result<()>,
{
    if pending_tasks.remove(task_id).is_none() {
        return Ok(());
    }
    unresolved_deps.remove(task_id);
    if skipped_tasks.insert(task_id.to_string()) {
        log(format!(
            "{graph_name}-skip {} skipped={}/{} {}",
            task_id,
            skipped_tasks.len(),
            total_tasks,
            skip_reason
        ))?;
    }
    if let Some(dependents) = reverse_deps.remove(task_id) {
        for dependent in dependents {
            skip_pending_graph_task(
                graph_name,
                &dependent,
                format!("failed_dep={task_id}"),
                total_tasks,
                pending_tasks,
                unresolved_deps,
                reverse_deps,
                skipped_tasks,
                log,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn enqueue_graph_task<K, Log>(
    graph_name: &str,
    task: GraphScheduledTask<K>,
    total_tasks: usize,
    known_task_ids: &mut BTreeSet<String>,
    completed_ids: &BTreeSet<String>,
    failed_ids: &BTreeSet<String>,
    failed_scopes: &BTreeSet<String>,
    skipped_tasks: &mut BTreeSet<String>,
    pending_tasks: &mut BTreeMap<String, GraphScheduledTask<K>>,
    unresolved_deps: &mut BTreeMap<String, usize>,
    reverse_deps: &mut BTreeMap<String, Vec<String>>,
    ready_tasks: &mut VecDeque<String>,
    failure_policy: &GraphFailurePolicy,
    log: &mut Log,
) -> anyhow::Result<()>
where
    Log: FnMut(String) -> anyhow::Result<()>,
{
    if !known_task_ids.insert(task.id.clone()) {
        bail!("{graph_name} duplicate dynamic task id {}", task.id);
    }

    let task_id = task.id.clone();
    let failed_dep = task
        .deps
        .iter()
        .find(|dep| failed_ids.contains(*dep) || skipped_tasks.contains(*dep))
        .cloned();
    let failed_scope =
        graph_failed_scope(failure_policy, &task_id).filter(|scope| failed_scopes.contains(scope));
    let unresolved = task
        .deps
        .iter()
        .filter(|dep| !completed_ids.contains(*dep))
        .count();
    for dep in task.deps.iter().filter(|dep| !completed_ids.contains(*dep)) {
        reverse_deps
            .entry(dep.clone())
            .or_default()
            .push(task_id.clone());
    }
    unresolved_deps.insert(task_id.clone(), unresolved);
    if unresolved == 0 {
        ready_tasks.push_back(task_id.clone());
    }
    pending_tasks.insert(task_id.clone(), task);

    if let Some(failed_dep) = failed_dep {
        skip_pending_graph_task(
            graph_name,
            &task_id,
            format!("failed_dep={failed_dep}"),
            total_tasks,
            pending_tasks,
            unresolved_deps,
            reverse_deps,
            skipped_tasks,
            log,
        )?;
    } else if let Some(failed_scope) = failed_scope {
        skip_pending_graph_task(
            graph_name,
            &task_id,
            format!("failed_scope={failed_scope}"),
            total_tasks,
            pending_tasks,
            unresolved_deps,
            reverse_deps,
            skipped_tasks,
            log,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn skip_pending_graph_scope<K, Log>(
    graph_name: &str,
    scope: &str,
    total_tasks: usize,
    pending_tasks: &mut BTreeMap<String, GraphScheduledTask<K>>,
    unresolved_deps: &mut BTreeMap<String, usize>,
    reverse_deps: &mut BTreeMap<String, Vec<String>>,
    skipped_tasks: &mut BTreeSet<String>,
    failure_policy: &GraphFailurePolicy,
    log: &mut Log,
) -> anyhow::Result<()>
where
    Log: FnMut(String) -> anyhow::Result<()>,
{
    let scoped_ids = pending_tasks
        .keys()
        .filter(|task_id| graph_failed_scope(failure_policy, task_id).as_deref() == Some(scope))
        .cloned()
        .collect::<Vec<_>>();
    for task_id in scoped_ids {
        skip_pending_graph_task(
            graph_name,
            &task_id,
            format!("failed_scope={scope}"),
            total_tasks,
            pending_tasks,
            unresolved_deps,
            reverse_deps,
            skipped_tasks,
            log,
        )?;
    }
    Ok(())
}

fn run_weighted_task_graph_impl<K, V, RunTask, ExpandTask, Log>(
    graph_name: &str,
    pending_tasks: Vec<GraphScheduledTask<K>>,
    work_unit_budget: usize,
    mut log: Log,
    run_task: RunTask,
    mut expand_task: ExpandTask,
    failure_policy: GraphFailurePolicy,
) -> anyhow::Result<GraphRunOutcome<V>>
where
    K: Clone + Send + 'static,
    V: Clone + Send + Sync + 'static,
    RunTask: Fn(
            K,
            GraphReadMap<V>,
            GraphReadMap<Vec<NodeRecord>>,
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

    let mut known_task_ids = BTreeSet::<String>::new();
    let mut pending_by_id = BTreeMap::<String, GraphScheduledTask<K>>::new();
    let mut unresolved_deps = BTreeMap::<String, usize>::new();
    let mut reverse_deps = BTreeMap::<String, Vec<String>>::new();
    let mut ready_tasks = VecDeque::<String>::new();
    let (tx, rx) =
        crossbeam_channel::unbounded::<(String, usize, anyhow::Result<GraphTaskCompletion<V>>)>();
    let mut running_jobs = 0_usize;
    let mut running_units = 0_usize;
    let mut launched_tasks = 0_usize;
    let mut completed_tasks = 0_usize;
    let mut completed_ids = std::collections::BTreeSet::<String>::new();
    let task_values = Arc::new(RwLock::new(BTreeMap::<String, V>::new()));
    let task_node_records = Arc::new(RwLock::new(BTreeMap::<String, Vec<NodeRecord>>::new()));
    let mut failures = BTreeMap::<String, String>::new();
    let mut failed_ids = BTreeSet::<String>::new();
    let mut failed_scopes = BTreeSet::<String>::new();
    let mut skipped_tasks = BTreeSet::<String>::new();
    let mut worker_threads = BTreeMap::<String, thread::JoinHandle<anyhow::Result<()>>>::new();
    let mut running_task_kinds = BTreeMap::<String, K>::new();

    for task in pending_tasks {
        enqueue_graph_task(
            graph_name,
            task,
            total_tasks,
            &mut known_task_ids,
            &completed_ids,
            &failed_ids,
            &failed_scopes,
            &mut skipped_tasks,
            &mut pending_by_id,
            &mut unresolved_deps,
            &mut reverse_deps,
            &mut ready_tasks,
            &failure_policy,
            &mut log,
        )?;
    }

    while running_jobs > 0 || !pending_by_id.is_empty() {
        let mut launched_any = false;
        loop {
            let mut deferred_ready = Vec::new();
            let mut selected_task_id = None;
            while let Some(task_id) = ready_tasks.pop_front() {
                let Some(task) = pending_by_id.get(&task_id) else {
                    continue;
                };
                if unresolved_deps.get(&task_id).copied().unwrap_or(usize::MAX) != 0 {
                    continue;
                }
                if running_units + task.weight <= work_unit_budget {
                    selected_task_id = Some(task_id);
                    break;
                }
                deferred_ready.push(task_id);
            }
            for task_id in deferred_ready.into_iter().rev() {
                ready_tasks.push_front(task_id);
            }
            let Some(selected_task_id) = selected_task_id else {
                break;
            };
            let task = pending_by_id
                .remove(&selected_task_id)
                .with_context(|| format!("missing ready task {selected_task_id}"))?;
            unresolved_deps.remove(&selected_task_id);
            let task_id = task.id.clone();
            let task_weight = task.weight;
            running_task_kinds.insert(task_id.clone(), task.kind.clone());
            launched_tasks += 1;
            log(task_log_record(
                TaskLogEvent::Start,
                &task_id,
                graph_name,
                [
                    ("launched", launched_tasks.to_string()),
                    ("total", total_tasks.to_string()),
                    ("completed", completed_tasks.to_string()),
                    ("weight", task_weight.to_string()),
                    ("running_units", (running_units + task_weight).to_string()),
                    ("work_unit_budget", work_unit_budget.to_string()),
                ],
                None,
            ))?;
            let tx = tx.clone();
            let task_values_snapshot = GraphReadMap::new(Arc::clone(&task_values));
            let task_node_records_snapshot = GraphReadMap::new(Arc::clone(&task_node_records));
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
            if pending_by_id.is_empty() {
                break;
            }
            bail!("{graph_name} deadlock: no runnable tasks remain");
        }
        if !launched_any {
            // wait for a running task to free capacity or satisfy dependencies
        }

        let first_completion = loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(message) => break message,
                Err(RecvTimeoutError::Timeout) => {
                    log(format!(
                        "{graph_name}-wait running_jobs={} pending_tasks={} running_units={}/{}",
                        running_jobs,
                        pending_by_id.len(),
                        running_units,
                        work_unit_budget,
                    ))?;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("{graph_name} channel closed unexpectedly");
                }
            }
        };
        let mut completions = vec![first_completion];
        while let Ok(completion) = rx.try_recv() {
            completions.push(completion);
        }
        for (task_id, task_weight, result) in completions {
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
                    task_node_records
                        .write()
                        .expect("graph node records lock poisoned")
                        .insert(task_id.clone(), completion.node_records.clone());
                    task_values
                        .write()
                        .expect("graph values lock poisoned")
                        .insert(task_id.clone(), completion.value.clone());
                    completed_ids.insert(task_id.clone());
                    if let Some(dependents) = reverse_deps.remove(&task_id) {
                        for dependent in dependents {
                            let Some(unresolved) = unresolved_deps.get_mut(&dependent) else {
                                continue;
                            };
                            *unresolved = unresolved.saturating_sub(1);
                            if *unresolved == 0 {
                                ready_tasks.push_back(dependent);
                            }
                        }
                    }
                    let spawned_tasks = expand_task(
                        &task_id,
                        &task_kind,
                        &completion,
                        &task_values.read().expect("graph values lock poisoned"),
                        &task_node_records
                            .read()
                            .expect("graph node records lock poisoned"),
                    )?;
                    if !spawned_tasks.is_empty() {
                        let spawned_count = spawned_tasks.len();
                        total_tasks += spawned_count;
                        log(format!(
                            "{graph_name}-spawn {} count={} total_tasks={}",
                            task_id, spawned_count, total_tasks
                        ))?;
                        for spawned_task in spawned_tasks {
                            enqueue_graph_task(
                                graph_name,
                                spawned_task,
                                total_tasks,
                                &mut known_task_ids,
                                &completed_ids,
                                &failed_ids,
                                &failed_scopes,
                                &mut skipped_tasks,
                                &mut pending_by_id,
                                &mut unresolved_deps,
                                &mut reverse_deps,
                                &mut ready_tasks,
                                &failure_policy,
                                &mut log,
                            )?;
                        }
                    }
                    log(task_log_record(
                        TaskLogEvent::Complete,
                        &task_id,
                        graph_name,
                        [
                            ("status", "PASS".to_string()),
                            ("completed", completed_tasks.to_string()),
                            ("total", total_tasks.to_string()),
                            ("running_units", running_units.to_string()),
                            ("work_unit_budget", work_unit_budget.to_string()),
                        ],
                        Some(&completion.completion_detail),
                    ))?;
                }
                Err(err) => {
                    let error_text = log_error_chain(&err);
                    let detail = format!("error={error_text}");
                    log(task_log_record(
                        TaskLogEvent::Complete,
                        &task_id,
                        graph_name,
                        [
                            ("status", "FAIL".to_string()),
                            ("completed", completed_tasks.to_string()),
                            ("total", total_tasks.to_string()),
                            ("running_units", running_units.to_string()),
                            ("work_unit_budget", work_unit_budget.to_string()),
                        ],
                        Some(&detail),
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
                    if let Some(dependents) = reverse_deps.remove(&task_id) {
                        for dependent in dependents {
                            skip_pending_graph_task(
                                graph_name,
                                &dependent,
                                format!("failed_dep={task_id}"),
                                total_tasks,
                                &mut pending_by_id,
                                &mut unresolved_deps,
                                &mut reverse_deps,
                                &mut skipped_tasks,
                                &mut log,
                            )?;
                        }
                    }
                    if let Some(scope) = graph_failed_scope(&failure_policy, &task_id) {
                        if failed_scopes.contains(&scope) {
                            skip_pending_graph_scope(
                                graph_name,
                                &scope,
                                total_tasks,
                                &mut pending_by_id,
                                &mut unresolved_deps,
                                &mut reverse_deps,
                                &mut skipped_tasks,
                                &failure_policy,
                                &mut log,
                            )?;
                        }
                    }
                    failures.insert(task_id, error_text);
                }
            }
        }
    }

    let final_task_values = task_values
        .read()
        .expect("graph values lock poisoned")
        .clone();
    let final_task_node_records = task_node_records
        .read()
        .expect("graph node records lock poisoned")
        .clone();
    Ok(GraphRunOutcome {
        task_values: final_task_values,
        task_node_records: final_task_node_records,
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
