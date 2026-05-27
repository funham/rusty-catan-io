use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use catan_bots::bot::BotPolicy;
use catan_bots::{greedy::GreedyAgent, lazy::LazyAgent, random::RandomAgent};
use catan_core::{
    gameplay::game::{
        run::{GameResult, RunOptions},
        state::{SetupGameOptions, SetupGameState},
    },
    gameplay::primitives::player::PlayerId,
    gameplay::random::GameRandom,
};
use catan_runtime::{
    config::{self, BotConfig, FieldConfig, MatchConfig},
    run_stats::{GameRunStats, RunStatsObserver},
    simulation::SimulationHost,
    sync_host::OutputObserver,
};
use serde::Serialize;

#[cfg(feature = "bench-allocs")]
mod alloc_counters {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicU64, Ordering},
    };

    #[derive(Debug, Clone, Copy, serde::Serialize)]
    pub struct AllocationSummary {
        pub alloc_calls: u64,
        pub dealloc_calls: u64,
        pub realloc_calls: u64,
        pub alloc_bytes: u64,
        pub dealloc_bytes: u64,
        pub realloc_new_bytes: u64,
    }

    pub struct CountingAllocator;

    static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static DEALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static REALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    static REALLOC_NEW_BYTES: AtomicU64 = AtomicU64::new(0);

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            REALLOC_NEW_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    pub fn reset() {
        ALLOC_CALLS.store(0, Ordering::Relaxed);
        DEALLOC_CALLS.store(0, Ordering::Relaxed);
        REALLOC_CALLS.store(0, Ordering::Relaxed);
        ALLOC_BYTES.store(0, Ordering::Relaxed);
        DEALLOC_BYTES.store(0, Ordering::Relaxed);
        REALLOC_NEW_BYTES.store(0, Ordering::Relaxed);
    }

    pub fn snapshot() -> AllocationSummary {
        AllocationSummary {
            alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            dealloc_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
            realloc_calls: REALLOC_CALLS.load(Ordering::Relaxed),
            alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            dealloc_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
            realloc_new_bytes: REALLOC_NEW_BYTES.load(Ordering::Relaxed),
        }
    }
}

#[cfg(feature = "bench-allocs")]
#[global_allocator]
static GLOBAL_ALLOCATOR: alloc_counters::CountingAllocator = alloc_counters::CountingAllocator;

#[cfg(feature = "bench-allocs")]
use alloc_counters::AllocationSummary;

#[cfg(not(feature = "bench-allocs"))]
type AllocationSummary = ();

#[derive(Debug)]
struct Args {
    config: PathBuf,
    games: u64,
    seed: u64,
    seed_stride: u64,
    max_turns: Option<u64>,
    json_summary: bool,
    no_log: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            config: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("data/configurations/greedy_brawl.json"),
            games: 1,
            seed: 0,
            seed_stride: 1,
            max_turns: None,
            json_summary: false,
            no_log: false,
        }
    }
}

#[derive(Debug)]
struct GameOutcome {
    result: GameResult,
    stats: GameRunStats,
}

#[derive(Debug, Default, Serialize)]
struct ResultCounts {
    wins: u64,
    limits: u64,
    interruptions: u64,
}

#[derive(Debug, Default, Serialize)]
struct Totals {
    turns_started: u64,
    turns_ended: u64,
    decision_requests: u64,
    regular_actions: u64,
    builds: u64,
    bank_trades: u64,
    dev_cards_bought: u64,
    dev_cards_used: u64,
    dice_rolls: u64,
    resources_distributed: u64,
    player_discards: u64,
    robber_moves: u64,
    action_rejections: u64,
}

impl Totals {
    fn add_stats(&mut self, stats: GameRunStats) {
        self.turns_started += stats.turns_started;
        self.turns_ended += stats.turns_ended;
        self.decision_requests += stats.decision_requests;
        self.regular_actions += stats.regular_actions;
        self.builds += stats.builds;
        self.bank_trades += stats.bank_trades;
        self.dev_cards_bought += stats.dev_cards_bought;
        self.dev_cards_used += stats.dev_cards_used;
        self.dice_rolls += stats.dice_rolls;
        self.resources_distributed += stats.resources_distributed;
        self.player_discards += stats.player_discards;
        self.robber_moves += stats.robber_moves;
        self.action_rejections += stats.action_rejections;
    }
}

#[derive(Debug, Serialize)]
struct Rates {
    simulations_per_sec: f64,
    turns_per_sec: f64,
    regular_actions_per_sec: f64,
    builds_per_sec: f64,
    decision_requests_per_sec: f64,
    legal_candidates_per_sec: Option<f64>,
}

#[derive(Debug, Serialize)]
struct Environment {
    git_commit: Option<String>,
    rustc: Option<String>,
    cargo: Option<String>,
    target_arch: &'static str,
    target_os: &'static str,
    os: Option<String>,
    profile: &'static str,
    bench_counters_enabled: bool,
    bench_allocs_enabled: bool,
}

#[derive(Debug, Serialize)]
struct Summary {
    config: String,
    games: u64,
    seed: u64,
    seed_stride: u64,
    max_turns: Option<u64>,
    elapsed_secs: f64,
    result_counts: ResultCounts,
    totals: Totals,
    rates: Rates,
    legal_counters: Option<LegalCounterSummary>,
    allocations: Option<AllocationSummary>,
    environment: Environment,
}

#[derive(Debug, Serialize)]
struct LegalCounterSummary {
    city_candidates: u64,
    settlement_candidates: u64,
    road_candidates: u64,
    dev_card_candidates: u64,
    roadbuild_candidates: u64,
    total_candidates: u64,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    if !args.no_log {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
    }

    let config = load_config(&args.config)?;
    validate_benchmark_config(&config)?;
    let effective_max_turns = args.max_turns.or(config.limits.max_turns);

    #[cfg(feature = "bench-counters")]
    catan_core::gameplay::game::legal::counters::reset();

    #[cfg(feature = "bench-allocs")]
    alloc_counters::reset();

    let started = Instant::now();
    let mut totals = Totals::default();
    let mut result_counts = ResultCounts::default();

    for game_index in 0..args.games {
        let seed = args
            .seed
            .wrapping_add(args.seed_stride.wrapping_mul(game_index));
        let outcome = run_one_game(&config, seed, args.max_turns)?;
        totals.add_stats(outcome.stats);
        match outcome.result {
            GameResult::Win(_) => result_counts.wins += 1,
            GameResult::LimitReached { .. } => result_counts.limits += 1,
            GameResult::Interrupted { .. } => result_counts.interruptions += 1,
        }
    }

    let elapsed = started.elapsed();
    let json_summary = args.json_summary;
    let summary = build_summary(args, effective_max_turns, elapsed, totals, result_counts);
    if summary.environment.bench_counters_enabled && summary.legal_counters.is_none() {
        return Err("bench counters were expected but not available".to_owned());
    }

    if summary.config.is_empty() {
        unreachable!("config path is always present");
    }

    if json_summary {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        print_human_summary(&summary);
    }

    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => parsed.config = next_path(&mut iter, "--config")?,
            "--games" => parsed.games = next_parse(&mut iter, "--games")?,
            "--seed" => parsed.seed = next_parse(&mut iter, "--seed")?,
            "--seed-stride" => parsed.seed_stride = next_parse(&mut iter, "--seed-stride")?,
            "--max-turns" => parsed.max_turns = Some(next_parse(&mut iter, "--max-turns")?),
            "--json-summary" => parsed.json_summary = true,
            "--no-log" => parsed.no_log = true,
            "--help" | "-h" => return Err(help_text()),
            other => return Err(format!("unknown argument: {other}\n\n{}", help_text())),
        }
    }

    if parsed.games == 0 {
        return Err("--games must be greater than zero".to_owned());
    }

    Ok(parsed)
}

fn next_path(iter: &mut impl Iterator<Item = String>, name: &str) -> Result<PathBuf, String> {
    iter.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} requires a value"))
}

fn next_parse<T: std::str::FromStr>(
    iter: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<T, String> {
    iter.next()
        .ok_or_else(|| format!("{name} requires a value"))?
        .parse::<T>()
        .map_err(|_| format!("{name} has an invalid value"))
}

fn help_text() -> String {
    [
        "usage: catan-bench [--config PATH] [--games N] [--seed N]",
        "                   [--seed-stride N] [--max-turns N]",
        "                   [--json-summary] [--no-log]",
    ]
    .join("\n")
}

fn load_config(path: &PathBuf) -> Result<MatchConfig, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {err}", path.display()))?;
    let mut config: MatchConfig = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse config {}: {err}", path.display()))?;
    config::resolve_paths(
        &mut config,
        path.parent().unwrap_or_else(|| std::path::Path::new(".")),
    );
    Ok(config)
}

fn validate_benchmark_config(config: &MatchConfig) -> Result<(), String> {
    if config.players.is_empty() {
        return Err("benchmark config must contain at least one player".to_owned());
    }
    if !config.observers.is_empty() {
        return Err("benchmark runner currently supports observer-free configs only".to_owned());
    }
    Ok(())
}

fn run_one_game(
    config: &MatchConfig,
    seed: u64,
    max_turns_override: Option<u64>,
) -> Result<GameOutcome, String> {
    let agents = build_agents(&config.players, seed);
    let init_state = build_initial_state(&config.field, config.players.len(), seed)?;
    let mut host = SimulationHost::new(
        init_state,
        agents,
        RunOptions {
            max_turns: max_turns_override.or(config.limits.max_turns),
            max_invalid_actions: config.limits.max_invalid_actions,
            random: GameRandom::seeded(seed),
        },
    );
    let mut stats_observer = RunStatsObserver::new();
    let mut observers: [&mut dyn OutputObserver; 1] = [&mut stats_observer];
    let result = host.run_observed(&mut observers);

    Ok(GameOutcome {
        result,
        stats: stats_observer.stats(),
    })
}

fn build_agents(players: &[BotConfig], seed: u64) -> Vec<Box<dyn BotPolicy>> {
    players
        .iter()
        .enumerate()
        .map(|(id, player)| {
            let player_id = PlayerId::try_from(id).expect("player count should fit in u8");
            match player {
                BotConfig::Lazy => Box::new(LazyAgent::new(player_id)) as Box<dyn BotPolicy>,
                BotConfig::Greedy => Box::new(GreedyAgent::new(player_id)) as Box<dyn BotPolicy>,
                BotConfig::Random => {
                    Box::new(RandomAgent::with_seed(player_id, agent_seed(seed, id)))
                        as Box<dyn BotPolicy>
                }
            }
        })
        .collect()
}

fn agent_seed(game_seed: u64, player_id: usize) -> u64 {
    game_seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((player_id as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(0x94D0_49BB_1331_11EB)
}

fn build_initial_state(
    config: &FieldConfig,
    player_count: usize,
    seed: u64,
) -> Result<SetupGameState, String> {
    match config {
        FieldConfig::Default => {
            let field = catan_core::gameplay::field::state::FieldBuildParam {
                n_players: player_count,
                ..Default::default()
            };
            Ok(SetupGameState::new_with_seed(field, seed))
        }
        FieldConfig::LayoutRef { path } => {
            let arrangement = catan_core::gameplay::field::ser::arrangement_from_json(path)
                .ok_or_else(|| format!("failed to read field layout {}", path.display()))?;
            Ok(SetupGameState::new_with_options(
                catan_core::gameplay::field::state::FieldBuildParam {
                    n_players: player_count,
                    arrangement,
                },
                SetupGameOptions {
                    random: GameRandom::seeded(seed),
                },
            ))
        }
    }
}

fn build_summary(
    args: Args,
    effective_max_turns: Option<u64>,
    elapsed: Duration,
    totals: Totals,
    result_counts: ResultCounts,
) -> Summary {
    let elapsed_secs = elapsed.as_secs_f64();
    let legal_counters = legal_counter_summary();
    let legal_total = legal_counters
        .as_ref()
        .map(|counters| counters.total_candidates as f64);

    Summary {
        config: args.config.display().to_string(),
        games: args.games,
        seed: args.seed,
        seed_stride: args.seed_stride,
        max_turns: effective_max_turns,
        elapsed_secs,
        rates: Rates {
            simulations_per_sec: rate(args.games, elapsed_secs),
            turns_per_sec: rate(totals.turns_started, elapsed_secs),
            regular_actions_per_sec: rate(totals.regular_actions, elapsed_secs),
            builds_per_sec: rate(totals.builds, elapsed_secs),
            decision_requests_per_sec: rate(totals.decision_requests, elapsed_secs),
            legal_candidates_per_sec: legal_total.map(|total| total / elapsed_secs),
        },
        totals,
        result_counts,
        legal_counters,
        allocations: allocation_summary(),
        environment: Environment {
            git_commit: command_output("git", &["rev-parse", "HEAD"]),
            rustc: command_output("rustc", &["-Vv"]),
            cargo: command_output("cargo", &["-V"]),
            target_arch: env::consts::ARCH,
            target_os: env::consts::OS,
            os: command_output("uname", &["-a"]).or_else(|| command_output("ver", &[])),
            profile: "release",
            bench_counters_enabled: cfg!(feature = "bench-counters"),
            bench_allocs_enabled: cfg!(feature = "bench-allocs"),
        },
    }
}

fn rate(count: u64, elapsed_secs: f64) -> f64 {
    if elapsed_secs == 0.0 {
        0.0
    } else {
        count as f64 / elapsed_secs
    }
}

fn legal_counter_summary() -> Option<LegalCounterSummary> {
    #[cfg(feature = "bench-counters")]
    {
        let counters = catan_core::gameplay::game::legal::counters::snapshot();
        let total_candidates = counters.city_candidates
            + counters.settlement_candidates
            + counters.road_candidates
            + counters.dev_card_candidates
            + counters.roadbuild_candidates;
        Some(LegalCounterSummary {
            city_candidates: counters.city_candidates,
            settlement_candidates: counters.settlement_candidates,
            road_candidates: counters.road_candidates,
            dev_card_candidates: counters.dev_card_candidates,
            roadbuild_candidates: counters.roadbuild_candidates,
            total_candidates,
        })
    }

    #[cfg(not(feature = "bench-counters"))]
    {
        None
    }
}

fn allocation_summary() -> Option<AllocationSummary> {
    #[cfg(feature = "bench-allocs")]
    {
        Some(alloc_counters::snapshot())
    }

    #[cfg(not(feature = "bench-allocs"))]
    {
        None
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()).filter(|text| !text.is_empty())
}

fn print_human_summary(summary: &Summary) {
    println!("config: {}", summary.config);
    println!(
        "games={} seed={} seed_stride={} elapsed={:.6}s",
        summary.games, summary.seed, summary.seed_stride, summary.elapsed_secs
    );
    println!(
        "results: wins={} limits={} interruptions={}",
        summary.result_counts.wins,
        summary.result_counts.limits,
        summary.result_counts.interruptions
    );
    println!(
        "totals: turns={} regular_actions={} decisions={} builds={} trades={} dice={}",
        summary.totals.turns_started,
        summary.totals.regular_actions,
        summary.totals.decision_requests,
        summary.totals.builds,
        summary.totals.bank_trades,
        summary.totals.dice_rolls
    );
    println!(
        "rates: simulations/s={:.3} turns/s={:.3} regular_actions/s={:.3} builds/s={:.3}",
        summary.rates.simulations_per_sec,
        summary.rates.turns_per_sec,
        summary.rates.regular_actions_per_sec,
        summary.rates.builds_per_sec
    );
    if let Some(counters) = &summary.legal_counters {
        println!(
            "legal_candidates: total={} city={} settlement={} road={} dev={} roadbuild={} rate/s={:.3}",
            counters.total_candidates,
            counters.city_candidates,
            counters.settlement_candidates,
            counters.road_candidates,
            counters.dev_card_candidates,
            counters.roadbuild_candidates,
            summary.rates.legal_candidates_per_sec.unwrap_or_default()
        );
    } else {
        println!("legal_candidates: disabled; rebuild with --features bench-counters");
    }
    print_allocation_summary(summary);
}

#[cfg(feature = "bench-allocs")]
fn print_allocation_summary(summary: &Summary) {
    if let Some(allocations) = &summary.allocations {
        println!(
            "allocations: alloc_calls={} realloc_calls={} dealloc_calls={} alloc_bytes={} realloc_new_bytes={} dealloc_bytes={}",
            allocations.alloc_calls,
            allocations.realloc_calls,
            allocations.dealloc_calls,
            allocations.alloc_bytes,
            allocations.realloc_new_bytes,
            allocations.dealloc_bytes
        );
    }
}

#[cfg(not(feature = "bench-allocs"))]
fn print_allocation_summary(_summary: &Summary) {
    println!("allocations: disabled; rebuild with --features bench-allocs");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn greedy_brawl_config() -> MatchConfig {
        load_config(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("data/configurations/greedy_brawl.json"),
        )
        .unwrap()
    }

    fn random_brawl_config() -> MatchConfig {
        load_config(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("data/configurations/random_brawl.json"),
        )
        .unwrap()
    }

    #[test]
    fn benchmark_config_accepts_random_agents() {
        let config = random_brawl_config();

        validate_benchmark_config(&config).unwrap();
        assert_eq!(build_agents(&config.players, 0).len(), config.players.len());
    }

    #[test]
    fn random_brawl_same_seed_produces_same_short_run_result_and_stats() {
        let config = random_brawl_config();

        let first = run_one_game(&config, 42, Some(5)).unwrap();
        let second = run_one_game(&config, 42, Some(5)).unwrap();

        assert_eq!(first.result, second.result);
        assert_eq!(first.stats, second.stats);
    }

    #[test]
    fn same_seed_produces_same_short_run_result_and_stats() {
        let config = greedy_brawl_config();

        let first = run_one_game(&config, 42, Some(5)).unwrap();
        let second = run_one_game(&config, 42, Some(5)).unwrap();

        assert_eq!(first.result, second.result);
        assert_eq!(first.stats, second.stats);
    }

    #[test]
    fn greedy_brawl_seed_zero_keeps_golden_summary() {
        let config = greedy_brawl_config();
        let outcome = run_one_game(&config, 0, None).unwrap();

        assert!(matches!(outcome.result, GameResult::Win(_)));
        assert_eq!(outcome.stats.turns_started, 100);
        assert_eq!(outcome.stats.turns_ended, 99);
        assert_eq!(outcome.stats.decision_requests, 790);
        assert_eq!(outcome.stats.regular_actions, 209);
        assert_eq!(outcome.stats.builds, 53);
        assert_eq!(outcome.stats.bank_trades, 32);
        assert_eq!(outcome.stats.dev_cards_bought, 25);
        assert_eq!(outcome.stats.dev_cards_used, 20);
        assert_eq!(outcome.stats.dice_rolls, 100);
        assert_eq!(outcome.stats.resources_distributed, 87);
        assert_eq!(outcome.stats.player_discards, 8);
        assert_eq!(outcome.stats.robber_moves, 27);
        assert_eq!(outcome.stats.action_rejections, 0);
    }

    #[test]
    #[ignore = "larger deterministic benchmark guard; run before behavior-sensitive refactors"]
    fn greedy_brawl_seed_batch_keeps_golden_summary() {
        let config = greedy_brawl_config();
        let mut totals = Totals::default();
        let mut result_counts = ResultCounts::default();

        for seed in 0..5 {
            let outcome = run_one_game(&config, seed, None).unwrap();
            totals.add_stats(outcome.stats);
            match outcome.result {
                GameResult::Win(_) => result_counts.wins += 1,
                GameResult::LimitReached { .. } => result_counts.limits += 1,
                GameResult::Interrupted { .. } => result_counts.interruptions += 1,
            }
        }

        assert_eq!(result_counts.wins, 5);
        assert_eq!(result_counts.limits, 0);
        assert_eq!(result_counts.interruptions, 0);
        assert_eq!(totals.turns_started, 533);
        assert_eq!(totals.turns_ended, 528);
        assert_eq!(totals.decision_requests, 1879);
        assert_eq!(totals.regular_actions, 1081);
        assert_eq!(totals.builds, 232);
        assert_eq!(totals.bank_trades, 215);
        assert_eq!(totals.dev_cards_bought, 106);
        assert_eq!(totals.dev_cards_used, 82);
        assert_eq!(totals.dice_rolls, 532);
        assert_eq!(totals.resources_distributed, 458);
        assert_eq!(totals.player_discards, 39);
        assert_eq!(totals.robber_moves, 133);
        assert_eq!(totals.action_rejections, 0);
    }

    #[test]
    #[ignore = "100-game deterministic macro guard; run before major optimization stages"]
    fn greedy_brawl_hundred_game_batch_keeps_golden_summary() {
        let config = greedy_brawl_config();
        let mut totals = Totals::default();
        let mut result_counts = ResultCounts::default();

        for seed in 0..100 {
            let outcome = run_one_game(&config, seed, None).unwrap();
            totals.add_stats(outcome.stats);
            match outcome.result {
                GameResult::Win(_) => result_counts.wins += 1,
                GameResult::LimitReached { .. } => result_counts.limits += 1,
                GameResult::Interrupted { .. } => result_counts.interruptions += 1,
            }
        }

        assert_eq!(result_counts.wins, 100);
        assert_eq!(result_counts.limits, 0);
        assert_eq!(result_counts.interruptions, 0);
        assert_eq!(totals.turns_started, 10327);
        assert_eq!(totals.turns_ended, 10227);
        assert_eq!(totals.decision_requests, 35579);
        assert_eq!(totals.regular_actions, 19974);
        assert_eq!(totals.builds, 4269);
        assert_eq!(totals.bank_trades, 3499);
        assert_eq!(totals.dev_cards_bought, 1979);
        assert_eq!(totals.dev_cards_used, 1485);
        assert_eq!(totals.dice_rolls, 10317);
        assert_eq!(totals.resources_distributed, 8625);
        assert_eq!(totals.player_discards, 535);
        assert_eq!(totals.robber_moves, 2754);
        assert_eq!(totals.action_rejections, 0);
    }
}
