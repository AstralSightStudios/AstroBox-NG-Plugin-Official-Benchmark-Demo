use crate::astrobox::psys_host::{self, ui};
use crate::benchmark::{self, BenchPhase, BenchStepStatus, ProgressUpdate};
use std::sync::{Mutex, OnceLock};

pub const START_BENCH_EVENT: &str = "benchmark_start";

struct UiState {
    root_element_id: Option<String>,
    running: bool,
    progress_done: usize,
    progress_total: usize,
    status: String,
    chunk_index: usize,
    chunk_total: usize,
    result_lines: Vec<String>,
    result_json: Option<String>,
}

#[derive(Clone)]
struct UiSnapshot {
    running: bool,
    progress_done: usize,
    progress_total: usize,
    status: String,
    chunk_index: usize,
    chunk_total: usize,
    result_lines: Vec<String>,
    result_json: Option<String>,
}

static UI_STATE: OnceLock<Mutex<UiState>> = OnceLock::new();

fn ui_state() -> &'static Mutex<UiState> {
    UI_STATE.get_or_init(|| {
        Mutex::new(UiState {
            root_element_id: None,
            running: false,
            progress_done: 0,
            progress_total: benchmark::TOTAL_STEPS,
            status: "等待开始".to_string(),
            chunk_index: 0,
            chunk_total: 0,
            result_lines: Vec::new(),
            result_json: None,
        })
    })
}

fn snapshot_from(state: &UiState) -> UiSnapshot {
    UiSnapshot {
        running: state.running,
        progress_done: state.progress_done,
        progress_total: state.progress_total,
        status: state.status.clone(),
        chunk_index: state.chunk_index,
        chunk_total: state.chunk_total,
        result_lines: state.result_lines.clone(),
        result_json: state.result_json.clone(),
    }
}

fn update_state_and_render<F>(update: F)
where
    F: FnOnce(&mut UiState),
{
    let (root, snapshot) = {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut state);
        let root = state.root_element_id.clone();
        let snapshot = snapshot_from(&state);
        (root, snapshot)
    };

    if let Some(root) = root {
        psys_host::ui::render(&root, build_main_ui(&snapshot));
    }
}

fn format_progress_status(update: &ProgressUpdate) -> String {
    let phase = match update.phase {
        BenchPhase::Warmup => "预热",
        BenchPhase::Measure => "测试",
    };
    let status = match update.status {
        BenchStepStatus::Started => "开始",
        BenchStepStatus::Finished => "完成",
        BenchStepStatus::Chunk => "进行中",
    };
    format!(
        "{} {} {}/{} {}",
        update.bench_id, phase, update.index, update.total, status
    )
}

fn effective_note() -> Option<String> {
    if benchmark::EFFECTIVE_N1 != benchmark::BENCH_N1
        || benchmark::EFFECTIVE_N2 != benchmark::BENCH_N2
    {
        Some(format!(
            " (effective n1={} n2={} maxChunks={})",
            benchmark::EFFECTIVE_N1,
            benchmark::EFFECTIVE_N2,
            benchmark::MAX_CHUNKS
        ))
    } else {
        None
    }
}

fn build_result_lines(result: &benchmark::BenchmarkResult) -> Vec<String> {
    let note = effective_note().unwrap_or_default();
    vec![
        format!(
            "参数: --seed {} --n1 {} --n2 {} --warmup {} --repeats {}{}",
            benchmark::BENCH_SEED,
            benchmark::BENCH_N1,
            benchmark::BENCH_N2,
            benchmark::BENCH_WARMUP,
            benchmark::BENCH_REPEATS,
            note
        ),
        format!("{} digest: {:016x}", result.t1.id, result.t1.digest),
        format!(
            "{} ms: min {:.3}, p50 {:.3}, p95 {:.3}, max {:.3}",
            result.t1.id,
            result.t1.stats.min,
            result.t1.stats.p50,
            result.t1.stats.p95,
            result.t1.stats.max
        ),
        format!("{} digest: {:016x}", result.t2.id, result.t2.digest),
        format!(
            "{} ms: min {:.3}, p50 {:.3}, p95 {:.3}, max {:.3}",
            result.t2.id,
            result.t2.stats.min,
            result.t2.stats.p50,
            result.t2.stats.p95,
            result.t2.stats.max
        ),
        format!("final_digest: {:016x}", result.final_digest),
    ]
}

fn run_benchmark_with_ui() {
    let (root, snapshot) = {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.running {
            return;
        }
        state.running = true;
        state.progress_done = 0;
        state.progress_total = benchmark::TOTAL_STEPS;
        state.status = "准备测试...".to_string();
        state.chunk_index = 0;
        state.chunk_total = 0;
        state.result_lines.clear();
        state.result_json = None;
        let root = state.root_element_id.clone();
        let snapshot = snapshot_from(&state);
        (root, snapshot)
    };

    if let Some(root) = root {
        psys_host::ui::render(&root, build_main_ui(&snapshot));
    }

    let result = benchmark::run_benchmark(|update| {
        let status = format_progress_status(&update);
        update_state_and_render(|state| {
            state.status = status;
            state.progress_done = update.completed_steps;
            state.progress_total = update.total_steps;
            state.chunk_index = update.chunk_index;
            state.chunk_total = update.chunk_total;
        });
    });

    let result_lines = build_result_lines(&result);
    update_state_and_render(|state| {
        state.running = false;
        state.progress_done = state.progress_total;
        state.status = "测试完成".to_string();
        state.result_lines = result_lines;
        state.result_json = Some(result.json);
    });
}

pub fn ui_event_processor(evtype: ui::Event, event: &str) {
    match evtype {
        ui::Event::Click => match event {
            START_BENCH_EVENT => run_benchmark_with_ui(),
            _ => {}
        },
        _ => {}
    }
}

fn build_main_ui(snapshot: &UiSnapshot) -> ui::Element {
    let title_text = "AstroBox Benchmark";
    let note = effective_note().unwrap_or_default();
    let subtitle_text = format!(
        "固定参数: --seed {} --n1 {} --n2 {} --warmup {} --repeats {}{}",
        benchmark::BENCH_SEED,
        benchmark::BENCH_N1,
        benchmark::BENCH_N2,
        benchmark::BENCH_WARMUP,
        benchmark::BENCH_REPEATS,
        note
    );

    let title = ui::Element::new(ui::ElementType::P, Some(title_text))
        .size(28)
        .margin_bottom(4);

    let subtitle = ui::Element::new(ui::ElementType::P, Some(subtitle_text.as_str()))
        .size(14)
        .text_color("#666666")
        .margin_bottom(12);

    let button_label = if snapshot.running { "测试中..." } else { "开始测试" };
    let mut start_button = ui::Element::new(ui::ElementType::Button, Some(button_label))
        .bg(if snapshot.running { "#9c9c9c" } else { "#14b86a" })
        .text_color("#ffffff")
        .padding(12)
        .radius(8)
        .margin_bottom(12);

    if snapshot.running {
        start_button = start_button.disabled();
    } else {
        start_button = start_button.on(ui::Event::Click, START_BENCH_EVENT);
    }

    let percent = if snapshot.progress_total > 0 {
        (snapshot.progress_done as f64 / snapshot.progress_total as f64) * 100.0
    } else {
        0.0
    };
    let progress_text = format!(
        "进度: {}/{} ({:.1}%)",
        snapshot.progress_done, snapshot.progress_total, percent
    );
    let progress = ui::Element::new(ui::ElementType::P, Some(progress_text.as_str()))
        .size(16)
        .margin_bottom(6);

    let status = ui::Element::new(ui::ElementType::P, Some(snapshot.status.as_str()))
        .size(14)
        .text_color("#444444")
        .margin_bottom(6);

    let chunk_text = if snapshot.running && snapshot.chunk_total > 0 {
        format!("Chunk: {}/{}", snapshot.chunk_index, snapshot.chunk_total)
    } else {
        "Chunk: -".to_string()
    };
    let chunk = ui::Element::new(ui::ElementType::P, Some(chunk_text.as_str()))
        .size(14)
        .text_color("#444444")
        .margin_bottom(12);

    let mut results_container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .align_start();

    if snapshot.result_lines.is_empty() && snapshot.result_json.is_none() {
        results_container = results_container.child(
            ui::Element::new(ui::ElementType::P, Some("结果会在这里显示。"))
                .size(14)
                .text_color("#777777"),
        );
    } else {
        for line in &snapshot.result_lines {
            results_container = results_container.child(
                ui::Element::new(ui::ElementType::P, Some(line.as_str()))
                    .size(14)
                    .margin_bottom(4),
            );
        }
        if let Some(json) = &snapshot.result_json {
            let json_label = ui::Element::new(ui::ElementType::P, Some("JSON:"))
                .size(14)
                .margin_top(8);
            let json_text = ui::Element::new(ui::ElementType::P, Some(json.as_str()))
                .size(12)
                .text_color("#555555");
            results_container = results_container.child(json_label).child(json_text);
        }
    }

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .justify_start()
        .align_start()
        .padding(16)
        .child(title)
        .child(subtitle)
        .child(start_button)
        .child(progress)
        .child(status)
        .child(chunk)
        .child(results_container)
}

pub fn render_main_ui(element_id: &str) {
    let (root, snapshot) = {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id = Some(element_id.to_string());
        let root = state.root_element_id.clone();
        let snapshot = snapshot_from(&state);
        (root, snapshot)
    };

    if let Some(root) = root {
        psys_host::ui::render(&root, build_main_ui(&snapshot));
    }
}
