//! Oh Hell web server. Mirrors the structure of `euchre_server`: an
//! Actix-web app that renders Maud HTML and uses htmx for polling +
//! form submission. The bot is a PIMCTS + open-hand-solver agent — no
//! pre-trained weights are required.

use std::{collections::HashMap, fs::OpenOptions, sync::Mutex};

use actix_web::{middleware::Logger, web, App, HttpResponse, HttpServer};
use card_platypus::{
    agents::Agent,
    algorithms::{
        cfres::{CFRES, OH_MAX_ACTIONS},
        gomcts_transformer::{
            forward_histories_batch_tch, masked_policy, oh_hell::OhHellTokenizer,
            GoMctsTransformerTch, InferenceMode, Tokenizer, TransformerConfig,
        },
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
};
use games::{
    actions,
    gamestates::oh_hell::{OHPhase, OhHell, OhHellGameState},
    istate::IStateKey,
    Action, GameState,
};
use log::{info, set_max_level, LevelFilter};
use rand::{rng, rngs::StdRng, SeedableRng};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};
use uuid::Uuid;

mod game_data;
mod html;

pub(crate) use game_data::{GameData, GameProcessingState};

const SERVER_HOST: &str = "0.0.0.0";
const SERVER_PORT: u16 = 4001;
const LOG_FILE: &str = "oh_hell_server.log";

/// Player counts this binary serves. 3-player runs the canonical
/// 10→1→10 schedule; 4-player is capped at 7-card hands by the game
/// engine's 64-slot `IStateKey` budget (`max_tricks_for(4) = 7`), so it
/// runs 7→1→7.
pub(crate) const SUPPORTED_PLAYERS: [usize; 2] = [3, 4];
pub(crate) const DEFAULT_PLAYERS: usize = 3;
/// Maximum human seats per game. The rest are bot-controlled (a
/// 3-player game with 3 humans has no bots at all).
pub(crate) const MAX_HUMANS: usize = 3;
/// PIMCTS rollout count per bot decision. Small enough to be quick on
/// every move yet large enough that the bot looks competent.
const BOT_ROLLOUTS: usize = 30;

/// The canonical Wikipedia hand-size schedule for a player count: deal
/// the maximum hand size, decrement to 1, then ascend back up. 3p: 10
/// max → 19 hands; 4p: 7 max (engine `IStateKey` cap) → 13 hands. The
/// final cumulative score (under "common scoring") determines the
/// winner.
pub fn default_hand_sequence(num_players: usize) -> Vec<usize> {
    let max = games::gamestates::oh_hell::max_tricks_for(num_players).min(10);
    let mut seq: Vec<usize> = (1..=max).rev().collect();
    seq.extend(2..=max);
    seq
}

/// Per-(player-count, hand-size) strategy the deployment WANTS, from
/// the R-NaD tournament evals (plans/rnad-implementation.md): serve
/// R-NaD where it outperforms both PIMCTS and the CFR bid weights, CFR
/// where it doesn't, PIMCTS where neither stronger option exists.
/// `strategy_for_hand_size` reports what the running process actually
/// loaded — a desired agent whose weights are missing on disk falls
/// back (R-NaD → CFR → PIMCTS) with a startup warning.
fn desired_strategy(num_players: usize, n_tricks: usize) -> &'static str {
    match (num_players, n_tricks) {
        // 3p, 1-trick hands are nearly pure bidding — exactly what the
        // CFR bid weights solved. R-NaD only ties them there
        // (+0.03±0.08 per hand, n=3000) while clearly beating PIMCTS,
        // so per the outperform-both-or-CFR rule, CFR keeps t1.
        (3, 1) => "CFR",
        // 3p: rnad_best beats PIMCTS-50 at every other size
        // (+0.68..+1.51 per hand, ≥3.5σ at n=300/t; pooled +1.19 over
        // t1-10) and the CFR bid weights at t2-5 (+0.32..+1.43, ≥4σ at
        // n=3000).
        (3, 2..=10) => "R-NaD",
        // 4p: rnad4_best beats PIMCTS-50 at every hand size incl. t1
        // (+0.63..+2.26 per hand, n=300/t; entry 8) and no 4p CFR
        // weights exist.
        (4, 1..=7) => "R-NaD",
        _ => "PIMCTS",
    }
}

/// The strategy actually being served, resolved at startup from
/// `desired_strategy` ∩ weights-on-disk. Outer index `num_players - 3`
/// (3p, 4p), inner index hand size 1..=10.
static STRATEGY_TABLE: std::sync::OnceLock<[[&'static str; 11]; 2]> = std::sync::OnceLock::new();

/// Look up the bot strategy in use for a given player count and hand
/// size (shown on the landing page).
pub fn strategy_for_hand_size(num_players: usize, n_tricks: usize) -> &'static str {
    STRATEGY_TABLE
        .get()
        .map(|t| t[num_players.clamp(3, 4) - 3][n_tricks.clamp(1, 10)])
        .unwrap_or("PIMCTS")
}

type OhCfres = CFRES<OhHellGameState, OH_MAX_ACTIONS, u64>;

/// Greedy-LM transformer agent (R-NaD checkpoint). One forward pass per
/// decision via the LM head masked to legal actions — CPU-friendly
/// (~tens of ms per move on the paper config, no CUDA required).
pub(crate) struct RnadAgent {
    net: GoMctsTransformerTch,
    tokenizer: OhHellTokenizer,
    rng: StdRng,
}

/// Near-greedy softmax temperature; matches the eval harnesses'
/// deployed-agent configuration (OH_TEMP=0.05).
const RNAD_TEMP: f64 = 0.05;

impl RnadAgent {
    fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let cfg = TransformerConfig::paper_default(
            OhHellTokenizer::VOCAB_SIZE,
            OhHellTokenizer::MAX_CONTEXT,
        );
        // cuda_if_available: GPU when present, plain CPU inference on
        // the GPU-less deploy target.
        let mut net = GoMctsTransformerTch::new(cfg, tch::Device::cuda_if_available())?;
        net.load_safetensors(path)?;
        Ok(Self {
            net,
            tokenizer: OhHellTokenizer,
            rng: StdRng::from_rng(&mut rng()),
        })
    }

    fn step(&mut self, gs: &OhHellGameState) -> Action {
        let mut legal = Vec::new();
        gs.legal_actions(&mut legal);
        if legal.len() == 1 {
            return legal[0];
        }
        let h: IStateKey = gs.istate_key(gs.cur_player());
        let probs = masked_policy(
            InferenceMode::LmSoftmax,
            0.0,
            RNAD_TEMP,
            |a| self.tokenizer.action_token(a),
            &h,
            &legal,
            |histories| forward_histories_batch_tch(&self.net, &self.tokenizer, &histories).ok(),
        );
        use rand::RngExt;
        let mut r: f64 = self.rng.random::<f64>();
        for (i, p) in probs.iter().enumerate() {
            r -= *p;
            if r <= 0.0 {
                return legal[i];
            }
        }
        legal[legal.len() - 1]
    }
}

/// The serving bot: dispatches each decision to the strongest available
/// agent for the current player count and hand size (see
/// `desired_strategy`).
pub(crate) struct Bot {
    pimcts: PIMCTSBot<OhHellGameState, OpenHandSolver<OhHellGameState>>,
    /// 3-player CFR bid weights, keyed by hand size (t1–5 on disk).
    cfr: HashMap<usize, OhCfres>,
    /// 3-player R-NaD checkpoint.
    rnad3: Option<RnadAgent>,
    /// 4-player R-NaD checkpoint (entry 8; same architecture, the
    /// tokenizer covers both player counts).
    rnad4: Option<RnadAgent>,
}

impl Bot {
    /// Production loadout: R-NaD checkpoints (3p + 4p) + 3p CFR bid
    /// weights + PIMCTS fallback. Missing weight files demote the
    /// affected configurations down the strategy ladder rather than
    /// failing startup.
    pub(crate) fn load_production() -> Self {
        let load_rnad = |env: &str, default: &str| -> Option<RnadAgent> {
            let path = std::env::var(env).unwrap_or_else(|_| default.to_string());
            match RnadAgent::load(std::path::Path::new(&path)) {
                Ok(agent) => Some(agent),
                Err(e) => {
                    log::warn!("R-NaD weights unavailable at {path}: {e:#}; falling back");
                    None
                }
            }
        };
        let rnad3 = load_rnad(
            "OH_RNAD_WEIGHTS",
            "/home/steven/card_platypus/gomcts/oh_hell/rnad_best.safetensors",
        );
        let rnad4 = load_rnad(
            "OH_RNAD4_WEIGHTS",
            "/home/steven/card_platypus/gomcts/oh_hell/rnad4_best.safetensors",
        );
        let cfr_base =
            std::env::var("OH_CFR_DIR").unwrap_or_else(|_| "/home/steven/card_platypus".into());
        let mut cfr = HashMap::new();
        for t in 1..=10usize {
            let dir = std::path::PathBuf::from(&cfr_base).join(format!("oh_hell.3p_{t}t_bid"));
            if dir.exists() {
                cfr.insert(t, OhCfres::new_oh_hell(3, t, 0, Some(&dir)));
            }
        }
        let bot = Self {
            pimcts: PIMCTSBot::new(
                BOT_ROLLOUTS,
                OpenHandSolver::new_oh_hell(),
                StdRng::from_rng(&mut rng()),
            ),
            cfr,
            rnad3,
            rnad4,
        };
        bot.publish_strategy_table();
        bot
    }

    /// Test/fallback loadout: PIMCTS at every hand size.
    pub(crate) fn pimcts_only(rollouts: usize) -> Self {
        let bot = Self {
            pimcts: PIMCTSBot::new(
                rollouts,
                OpenHandSolver::new_oh_hell(),
                StdRng::from_rng(&mut rng()),
            ),
            cfr: HashMap::new(),
            rnad3: None,
            rnad4: None,
        };
        bot.publish_strategy_table();
        bot
    }

    fn resolve_strategy(&self, num_players: usize, n_tricks: usize) -> &'static str {
        let desired = desired_strategy(num_players, n_tricks);
        let rnad_loaded = match num_players {
            3 => self.rnad3.is_some(),
            4 => self.rnad4.is_some(),
            _ => false,
        };
        // Demote down the ladder when the desired weights didn't load.
        // CFR weights are 3-player only.
        if desired == "R-NaD" && rnad_loaded {
            "R-NaD"
        } else if desired != "PIMCTS" && num_players == 3 && self.cfr.contains_key(&n_tricks) {
            "CFR"
        } else {
            "PIMCTS"
        }
    }

    fn publish_strategy_table(&self) {
        let mut table = [["PIMCTS"; 11]; 2];
        for (npi, row) in table.iter_mut().enumerate() {
            for (t, entry) in row.iter_mut().enumerate().skip(1) {
                *entry = self.resolve_strategy(npi + 3, t);
            }
        }
        let _ = STRATEGY_TABLE.set(table);
        info!("bot strategy per hand size, 3p (1..=10): {:?}", &table[0][1..]);
        info!("bot strategy per hand size, 4p (1..=7): {:?}", &table[1][1..=7]);
    }

    pub(crate) fn step(&mut self, gs: &OhHellGameState) -> Action {
        let np = gs.num_players();
        match self.resolve_strategy(np, gs.n_tricks()) {
            "R-NaD" => match np {
                4 => self.rnad4.as_mut().expect("resolved R-NaD implies loaded").step(gs),
                _ => self.rnad3.as_mut().expect("resolved R-NaD implies loaded").step(gs),
            },
            "CFR" => self
                .cfr
                .get_mut(&gs.n_tricks())
                .expect("resolved CFR implies loaded")
                .step(gs),
            _ => self.pimcts.step(gs),
        }
    }
}

pub(crate) struct AppState {
    pub(crate) games: Mutex<HashMap<Uuid, GameData>>,
    pub(crate) bot: Mutex<Bot>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            games: Default::default(),
            bot: Mutex::new(Bot::load_production()),
        }
    }
}

pub(crate) fn handle_ready_clear(
    game_data: &mut GameData,
    player_id: usize,
) -> Result<(), HttpResponse> {
    match &mut game_data.display_state {
        GameProcessingState::WaitingTrickClear { ready_players }
        | GameProcessingState::WaitingBidClear { ready_players }
        | GameProcessingState::WaitingHandClear { ready_players } => {
            if !ready_players.contains(&player_id) {
                ready_players.push(player_id);
            }
            Ok(())
        }
        _ => Err(HttpResponse::BadRequest().body(format!(
            "can't ready to clear in current state: {:?}",
            game_data.display_state
        ))),
    }
}

pub(crate) fn handle_take_action(
    game_data: &mut GameData,
    a: Action,
    player_id: usize,
) -> Result<(), HttpResponse> {
    if !matches!(
        game_data.display_state,
        GameProcessingState::WaitingHumanMove
    ) {
        return Err(HttpResponse::BadRequest().body(format!(
            "cannot take action in current state: {:?}",
            game_data.display_state
        )));
    }

    let legal = actions!(game_data.gs);
    if !legal.contains(&a) {
        return Err(HttpResponse::BadRequest().body("illegal action attempted"));
    }

    let seat = match game_data
        .players
        .iter()
        .position(|x| *x == Some(player_id))
    {
        Some(x) => x,
        None => {
            return Err(HttpResponse::BadRequest()
                .body("attempted to make a move for a player not registered to this game"))
        }
    };

    if game_data.gs.cur_player() != seat {
        return Err(HttpResponse::BadRequest().body(format!(
            "attempted action on wrong players turn. Current player is: {}",
            game_data.gs.cur_player(),
        )));
    }

    game_data.gs.apply_action(a);
    Ok(())
}

pub(crate) fn handle_register_player(
    game_data: &mut GameData,
    player_id: usize,
) -> Result<(), HttpResponse> {
    if game_data.players.contains(&Some(player_id)) {
        return Ok(());
    }
    let humans = game_data.players.iter().flatten().count();
    if humans >= game_data.num_humans {
        return Err(HttpResponse::Forbidden().body("game already has all human seats filled"));
    }
    // Drop the new human into the first empty seat. Other seats stay
    // bot-controlled. With 3 players and num_humans=2 this gives
    // seats [Some(creator), Some(joiner), None] — two humans across
    // from one bot.
    let slot = game_data
        .players
        .iter()
        .position(|x| x.is_none())
        .expect("must have free seat when humans < num_humans");
    game_data.players[slot] = Some(player_id);
    Ok(())
}

/// Drive the state machine forward, applying bot moves as needed, until
/// we land in a state that requires user input (or the game ends).
pub(crate) fn progress_game(
    game_data: &mut GameData,
    bot: &Mutex<Bot>,
    game_id: &Uuid,
) {
    use GameProcessingState::*;

    loop {
        let new_state = match &game_data.display_state {
            WaitingPlayerJoin { min_players } => {
                if game_data.players.iter().filter(|x| x.is_some()).count() < *min_players {
                    WaitingPlayerJoin {
                        min_players: *min_players,
                    }
                } else {
                    advance_state_after_action(game_data)
                }
            }
            WaitingHumanMove | WaitingMachineMoves => advance_state_after_action(game_data),
            WaitingBidClear { ready_players }
            | WaitingTrickClear { ready_players }
            | WaitingHandClear { ready_players } => {
                let humans = game_data.players.iter().flatten().count();
                if ready_players.len() < humans {
                    game_data.display_state.clone()
                } else if matches!(game_data.display_state, WaitingHandClear { .. }) {
                    // Finalise scores for the just-finished hand, then
                    // start the next hand from the schedule. If we've
                    // played the last hand in the schedule, the game is
                    // over and the highest cumulative score wins.
                    finalise_hand(game_data, game_id);
                    game_data.hand_idx += 1;
                    if game_data.hand_idx >= game_data.hand_sequence.len() {
                        info!(
                            "game over|id|{}|scores|{:?}|players|{:?}",
                            game_id, game_data.scores, game_data.players
                        );
                        GameOver
                    } else {
                        let next_size = game_data.hand_sequence[game_data.hand_idx];
                        game_data.gs = new_hand(game_data.players.len(), next_size);
                        next_seat_state(game_data)
                    }
                } else {
                    // BidClear / TrickClear cleared: don't re-enter the
                    // detection paths in advance_state_after_action,
                    // they'd flip us right back. Just hand off to whichever
                    // seat is next.
                    next_seat_state(game_data)
                }
            }
            GameOver => GameOver,
        };
        game_data.display_state = new_state;

        if !matches!(game_data.display_state, WaitingMachineMoves) {
            break;
        }

        // Bot's turn. Drive the chance phases too — a fresh hand starts
        // in DealHands which is technically a chance node, not the
        // bot's turn, but it's not the human's either.
        if game_data.gs.is_chance_node() {
            use rand::seq::IndexedRandom;
            let mut acts = Vec::new();
            game_data.gs.legal_actions(&mut acts);
            let a = *acts.choose(&mut rng()).unwrap();
            game_data.gs.apply_action(a);
        } else {
            let mut agent = bot.lock().unwrap();
            let a = agent.step(&game_data.gs);
            game_data.gs.apply_action(a);
        }
    }
}

/// Pick the next processing state given that the gamestate just had an
/// action applied to it. Detects bid-completion, trick-completion, and
/// hand-completion to pause for the UI.
fn advance_state_after_action(game_data: &GameData) -> GameProcessingState {
    use GameProcessingState::*;
    let gs = &game_data.gs;

    if gs.is_terminal() {
        return WaitingHandClear {
            ready_players: vec![],
        };
    }
    if gs.is_trick_over() {
        return WaitingTrickClear {
            ready_players: vec![],
        };
    }
    // Show bids once, when bidding has just finished and we're about to
    // start play. Detect: phase==Play AND no cards played yet.
    if gs.phase() == OHPhase::Play && gs.cards_played() == 0 {
        return WaitingBidClear {
            ready_players: vec![],
        };
    }
    next_seat_state(game_data)
}

/// Pick a state based purely on whose turn it is (no clear-state
/// detection). Used after a clear has been acknowledged or a fresh hand
/// has begun — those cases would otherwise re-fire the trick/bid
/// detection logic above.
fn next_seat_state(game_data: &GameData) -> GameProcessingState {
    use GameProcessingState::*;
    let gs = &game_data.gs;
    if gs.is_chance_node() {
        return WaitingMachineMoves;
    }
    match game_data.players[gs.cur_player()] {
        Some(_) => WaitingHumanMove,
        None => WaitingMachineMoves,
    }
}

/// At end-of-hand, add per-seat raw scores (common scoring: 1 point per
/// trick + 10 bonus if the bid matched exactly) to the cumulative
/// running totals. Delegates the formula to `OhHellGameState::raw_scores`
/// so the server and game state stay in sync.
fn finalise_hand(game_data: &mut GameData, game_id: &Uuid) {
    let gs = &game_data.gs;
    let np = gs.num_players();
    let raw = gs.raw_scores();
    for p in 0..np {
        game_data.scores[p] += raw[p] as usize;
    }
    info!(
        "hand ended|id|{}|bids|{:?}|tricks|{:?}|raw|{:?}|cumulative|{:?}|players|{:?}",
        game_id,
        gs.bids(),
        gs.tricks_won(),
        &raw[..np],
        game_data.scores,
        game_data.players
    );
}

pub(crate) fn new_hand(num_players: usize, n_tricks: usize) -> OhHellGameState {
    OhHell::new_state(num_players, n_tricks)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    set_max_level(LevelFilter::Trace);
    let config = ConfigBuilder::new().set_time_format_rfc3339().build();

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Debug,
            config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            config,
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(LOG_FILE)
                .expect("failed to open log file for writing"),
        ),
    ])
    .expect("failed to initialize logger");

    info!("starting oh_hell_server on {}:{}", SERVER_HOST, SERVER_PORT);
    let app_state = web::Data::new(AppState::default());

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(Logger::default())
            .configure(html::configure)
    })
    .bind((SERVER_HOST, SERVER_PORT))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    //! Fuzz-style integration test: simulate full games end-to-end via the
    //! same code paths the HTTP handlers use. Catches state-machine bugs
    //! before they hit production.
    use std::sync::Mutex;

    use card_platypus::algorithms::{open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot};
    use games::GameState;
    use rand::{rng, rngs::StdRng, SeedableRng};
    use uuid::Uuid;

    use crate::{
        handle_ready_clear, handle_take_action, html::render_game_view, new_hand, progress_game,
        Bot, GameData, GameProcessingState,
    };

    fn make_test_bot() -> Mutex<Bot> {
        // 1 rollout keeps the fuzz fast; the bot's policy quality is
        // not what we're testing here.
        Mutex::new(Bot::pimcts_only(1))
    }

    /// Test-only hand sequence: 3 → 2 → 1 → 2 → 3. Mirrors the shape of
    /// the production schedule (descend-then-ascend) without burning
    /// the wall-clock budget on full 10-card hands × 19 rounds.
    fn test_hand_sequence() -> Vec<usize> {
        vec![3, 2, 1, 2, 3]
    }

    fn play_random_game(bot: &Mutex<Bot>, human_id: usize, num_players: usize) {
        let game_id = Uuid::new_v4();
        let sequence = test_hand_sequence();
        let first_size = sequence[0];
        let mut gd = GameData::new(
            new_hand(num_players, first_size),
            human_id,
            1,
            num_players,
            sequence,
        );
        progress_game(&mut gd, bot, &game_id);

        for _ in 0..6000 {
            // Rendering runs on every HTTP response — include it so
            // "passes invalid input to renderer" bugs surface.
            let _ = render_game_view(&gd, human_id, &game_id).into_string();

            match &gd.display_state {
                GameProcessingState::WaitingHumanMove => {
                    let mut legal = Vec::new();
                    gd.gs.legal_actions(&mut legal);
                    assert!(!legal.is_empty(), "no legal actions for human turn");
                    let a = legal[rand::random::<u32>() as usize % legal.len()];
                    handle_take_action(&mut gd, a, human_id).expect("take action");
                }
                GameProcessingState::WaitingBidClear { .. }
                | GameProcessingState::WaitingTrickClear { .. }
                | GameProcessingState::WaitingHandClear { .. } => {
                    handle_ready_clear(&mut gd, human_id).expect("ready clear");
                }
                GameProcessingState::GameOver => return,
                GameProcessingState::WaitingMachineMoves
                | GameProcessingState::WaitingPlayerJoin { .. } => {}
            }
            progress_game(&mut gd, bot, &game_id);
        }
        panic!("game did not reach GameOver within iteration cap");
    }

    #[test]
    fn random_play_does_not_panic() {
        let bot = make_test_bot();
        for _ in 0..40 {
            play_random_game(&bot, 0, 3);
        }
        assert!(!bot.is_poisoned(), "bot mutex got poisoned during fuzz");
    }

    #[test]
    fn random_play_does_not_panic_4p() {
        let bot = make_test_bot();
        for _ in 0..40 {
            play_random_game(&bot, 0, 4);
        }
        assert!(!bot.is_poisoned(), "bot mutex got poisoned during fuzz");
    }
}
