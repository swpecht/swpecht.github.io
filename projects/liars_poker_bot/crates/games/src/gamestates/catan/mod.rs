//! Settlers of Catan (base game) for 2-4 players on the fixed beginner board.
//!
//! Rule scope (v1, aimed at agent training):
//! - Fixed beginner board layout (see `board.rs`), no board randomization.
//! - Bank and port trades only; no player-to-player trading.
//! - Dev cards are only playable after the dice roll (no pre-roll knight).
//! - All chance is expressed as uniform chance nodes so the framework's
//!   uniform sampling produces correct probabilities: dice are two separate
//!   d6 rolls, and card draws/steals are "slot" indices into a sorted
//!   multiset (see `actions.rs`).
//! - Games are capped at `MAX_ROLLS` dice rolls so random rollouts always
//!   terminate; at the cap the player with the most victory points wins.
//!
//! The state is a pure function of `(num_players, history)`, which makes
//! `undo` a pop-and-replay and keeps serialization small.

pub mod actions;
pub mod board;

use std::{
    collections::hash_map::DefaultHasher,
    fmt::Display,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::{
    istate::IStateKey, resample::ResampleFromInfoState, Action, Game, GameState, Player,
};

use self::actions::{
    CatanAction, BUY_DEV, CITY_BASE, DIE_ROLL_BASE, END_TURN, MAX_ACTION, MOVE_ROBBER_BASE,
    PICK_RESOURCE_BASE, PLAY_KNIGHT, PLAY_MONOPOLY, PLAY_ROAD_BUILDING, PLAY_YEAR_OF_PLENTY,
    ROAD_BASE, SETTLEMENT_BASE, STEAL_FROM_BASE, TRADE_BASE,
};
use self::board::{
    geometry, Port, Resource, Terrain, NUM_EDGES, NUM_HEXES, NUM_VERTICES, RESOURCES,
};

/// Games end after this many dice rolls; highest VP total wins. Random play
/// rarely reaches 10 VP, so the cap keeps rollouts bounded.
pub const MAX_ROLLS: u16 = 200;
pub const WIN_VP: u8 = 10;
const MAX_PLAYERS: usize = 4;

// Dev card types, also used as indices into dev-card count arrays.
pub const DEV_KNIGHT: usize = 0;
pub const DEV_ROAD_BUILDING: usize = 1;
pub const DEV_YEAR_OF_PLENTY: usize = 2;
pub const DEV_MONOPOLY: usize = 3;
pub const DEV_VICTORY_POINT: usize = 4;
const DEV_DECK_INIT: [u8; 5] = [14, 2, 2, 2, 5];

// Build costs, indexed by Resource: [brick, lumber, ore, grain, wool]
const ROAD_COST: [u8; 5] = [1, 1, 0, 0, 0];
const SETTLEMENT_COST: [u8; 5] = [1, 1, 0, 1, 1];
const CITY_COST: [u8; 5] = [0, 0, 3, 2, 0];
const DEV_COST: [u8; 5] = [0, 0, 1, 1, 1];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum CatanPhase {
    /// Setup: place a free settlement (snake order).
    SetupSettlement,
    /// Setup: place a free road next to the settlement just placed.
    SetupRoad,
    /// Chance: first die.
    Roll1,
    /// Chance: second die; the roll resolves when it lands.
    Roll2,
    /// A player with more than 7 cards discards one card at a time.
    Discard,
    /// The turn player moves the robber.
    MoveRobber,
    /// The turn player picks which adjacent player to rob (2+ candidates).
    StealChoice,
    /// Chance: which card is stolen (slot into the victim's hand).
    StealCard,
    /// Chance: which dev card is drawn (slot into the deck).
    DevDraw,
    /// Main phase: build, trade, play a dev card, or end the turn.
    Main,
    /// Free road placements from a Road Building card.
    FreeRoad,
    /// Take a resource from the bank (Year of Plenty, twice).
    PickYearOfPlenty,
    /// Name the resource to monopolize.
    PickMonopoly,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct PlayerState {
    resources: [u8; 5],
    /// Playable dev cards by type.
    dev: [u8; 5],
    /// Dev cards bought this turn (not playable until next turn).
    dev_new: [u8; 5],
    knights_played: u8,
    roads_left: u8,
    settlements_left: u8,
    cities_left: u8,
}

impl PlayerState {
    fn new() -> Self {
        Self {
            resources: [0; 5],
            dev: [0; 5],
            dev_new: [0; 5],
            knights_played: 0,
            roads_left: 15,
            settlements_left: 5,
            cities_left: 4,
        }
    }

    fn hand_total(&self) -> u8 {
        self.resources.iter().sum()
    }

    fn dev_total(&self) -> u8 {
        self.dev.iter().sum::<u8>() + self.dev_new.iter().sum::<u8>()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CatanGameState {
    num_players: usize,
    phase: CatanPhase,
    /// Whose turn it is (owner of pending chance nodes).
    turn_player: Player,
    /// Who acts now; differs from `turn_player` only while discarding.
    cur: Player,
    players: Vec<PlayerState>,
    /// 0 = empty, 1..=4 = settlement of player id-1, 5..=8 = city of player id-5.
    #[serde(with = "BigArray")]
    vertices: [u8; NUM_VERTICES],
    /// 0 = empty, 1..=4 = road of player id-1.
    #[serde(with = "BigArray")]
    edges: [u8; NUM_EDGES],
    robber: u8,
    bank: [u8; 5],
    /// Remaining dev cards by type.
    dev_deck: [u8; 5],
    longest_road: Option<u8>,
    largest_army: Option<u8>,
    dev_played_this_turn: bool,
    die1: u8,
    die2: u8,
    rolls: u16,
    /// Number of completed setup placements (settlement+road pairs).
    setup_idx: u8,
    pending_discards: [u8; MAX_PLAYERS],
    steal_victim: u8,
    free_roads: u8,
    yop_picks: u8,
    /// Vertex of the settlement just placed during setup.
    last_settlement: u8,
    game_over: bool,
    history: Vec<Action>,
}

pub struct Catan {}

impl Catan {
    pub fn new_state(num_players: usize) -> CatanGameState {
        assert!(
            (2..=MAX_PLAYERS).contains(&num_players),
            "catan supports 2-4 players"
        );
        CatanGameState {
            num_players,
            phase: CatanPhase::SetupSettlement,
            turn_player: 0,
            cur: 0,
            players: vec![PlayerState::new(); num_players],
            vertices: [0; NUM_VERTICES],
            edges: [0; NUM_EDGES],
            robber: geometry().desert_hex,
            bank: [19; 5],
            dev_deck: DEV_DECK_INIT,
            longest_road: None,
            largest_army: None,
            dev_played_this_turn: false,
            die1: 0,
            die2: 0,
            rolls: 0,
            setup_idx: 0,
            pending_discards: [0; MAX_PLAYERS],
            steal_victim: 0,
            free_roads: 0,
            yop_picks: 0,
            last_settlement: 0,
            game_over: false,
            history: Vec::new(),
        }
    }

    /// `Game::new` requires a function pointer, so dispatch over the
    /// supported player counts.
    pub fn game(num_players: usize) -> Game<CatanGameState> {
        let new_f: fn() -> CatanGameState = match num_players {
            2 => || Catan::new_state(2),
            3 => || Catan::new_state(3),
            4 => || Catan::new_state(4),
            _ => panic!("catan supports 2-4 players"),
        };
        Game {
            new: Box::new(new_f),
            max_players: num_players,
            max_actions: MAX_ACTION as usize,
        }
    }
}

/// Map a slot index into a multiset of counts to the item type it selects.
fn slot_to_type(counts: &[u8; 5], slot: u8) -> usize {
    let mut s = slot;
    for (i, &c) in counts.iter().enumerate() {
        if s < c {
            return i;
        }
        s -= c;
    }
    panic!("chance slot {slot} out of range for {counts:?}");
}

/// The lowest slot index selecting an item of type `t`, if any remain.
fn first_slot_of_type(counts: &[u8; 5], t: usize) -> Option<u8> {
    if counts[t] == 0 {
        return None;
    }
    Some(counts[..t].iter().sum())
}

impl CatanGameState {
    fn mark(p: Player) -> u8 {
        p as u8 + 1
    }

    /// (owner, is_city) of a vertex, if occupied.
    fn vertex_owner(&self, v: usize) -> Option<(Player, bool)> {
        match self.vertices[v] {
            0 => None,
            x if x <= 4 => Some(((x - 1) as Player, false)),
            x => Some(((x - 5) as Player, true)),
        }
    }

    fn can_afford(&self, p: Player, cost: &[u8; 5]) -> bool {
        self.players[p]
            .resources
            .iter()
            .zip(cost)
            .all(|(have, need)| have >= need)
    }

    fn pay(&mut self, p: Player, cost: &[u8; 5]) {
        for r in 0..5 {
            self.players[p].resources[r] -= cost[r];
            self.bank[r] += cost[r];
        }
    }

    fn can_place_settlement(&self, v: usize, p: Player, setup: bool) -> bool {
        if self.vertices[v] != 0 {
            return false;
        }
        let geo = geometry();
        // Distance rule: no building on an adjacent vertex.
        if geo.vertex_neighbors[v]
            .iter()
            .any(|&w| self.vertices[w as usize] != 0)
        {
            return false;
        }
        if setup {
            return true;
        }
        // Must connect to one of the player's roads.
        geo.vertex_edges[v]
            .iter()
            .any(|&e| self.edges[e as usize] == Self::mark(p))
    }

    fn can_place_road(&self, e: usize, p: Player) -> bool {
        if self.edges[e] != 0 {
            return false;
        }
        let geo = geometry();
        let (a, b) = geo.edge_vertices[e];
        for v in [a as usize, b as usize] {
            match self.vertex_owner(v) {
                Some((owner, _)) if owner == p => return true,
                // An opponent's building blocks connection through the vertex.
                Some(_) => continue,
                None => {
                    if geo.vertex_edges[v]
                        .iter()
                        .any(|&e2| e2 as usize != e && self.edges[e2 as usize] == Self::mark(p))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn any_legal_road(&self, p: Player) -> bool {
        (0..NUM_EDGES).any(|e| self.can_place_road(e, p))
    }

    /// Players adjacent to the robber's hex that the turn player can rob.
    fn robber_victims(&self) -> Vec<Player> {
        let geo = geometry();
        let mut victims = Vec::new();
        for &v in &geo.hex_vertices[self.robber as usize] {
            if let Some((owner, _)) = self.vertex_owner(v as usize) {
                if owner != self.turn_player
                    && self.players[owner].hand_total() > 0
                    && !victims.contains(&owner)
                {
                    victims.push(owner);
                }
            }
        }
        victims.sort();
        victims
    }

    fn trade_rate(&self, p: Player, give: Resource) -> u8 {
        let geo = geometry();
        let mut rate = 4;
        for v in 0..NUM_VERTICES {
            if let Some((owner, _)) = self.vertex_owner(v) {
                if owner != p {
                    continue;
                }
                match geo.vertex_port[v] {
                    Some(Port::TwoToOne(r)) if r == give => return 2,
                    Some(Port::ThreeToOne) => rate = 3.min(rate),
                    _ => {}
                }
            }
        }
        rate
    }

    /// Victory points visible to everyone (excludes unplayed VP dev cards).
    fn public_vp(&self, p: Player) -> u8 {
        let mut vp = 0;
        for v in 0..NUM_VERTICES {
            if let Some((owner, is_city)) = self.vertex_owner(v) {
                if owner == p {
                    vp += if is_city { 2 } else { 1 };
                }
            }
        }
        if self.longest_road == Some(p as u8) {
            vp += 2;
        }
        if self.largest_army == Some(p as u8) {
            vp += 2;
        }
        vp
    }

    fn vp(&self, p: Player) -> u8 {
        self.public_vp(p)
            + self.players[p].dev[DEV_VICTORY_POINT]
            + self.players[p].dev_new[DEV_VICTORY_POINT]
    }

    /// Length of the longest simple road path for a player. Opponent
    /// buildings block continuation through a vertex but may end a path.
    fn longest_road_len(&self, p: Player) -> u8 {
        let geo = geometry();
        let mark = Self::mark(p);
        let mut best = 0;
        for v in 0..NUM_VERTICES {
            for &e in &geo.vertex_edges[v] {
                if self.edges[e as usize] != mark {
                    continue;
                }
                let (a, b) = geo.edge_vertices[e as usize];
                let next = if a as usize == v { b } else { a };
                best = best.max(1 + self.road_dfs(next, 1u128 << e, mark));
            }
        }
        best
    }

    fn road_dfs(&self, v: u8, visited: u128, mark: u8) -> u8 {
        // Arrived at an opponent building: the path ends here.
        if let Some((owner, _)) = self.vertex_owner(v as usize) {
            if Self::mark(owner) != mark {
                return 0;
            }
        }
        let geo = geometry();
        let mut best = 0;
        for &e in &geo.vertex_edges[v as usize] {
            if visited & (1u128 << e) != 0 || self.edges[e as usize] != mark {
                continue;
            }
            let (a, b) = geo.edge_vertices[e as usize];
            let next = if a == v { b } else { a };
            best = best.max(1 + self.road_dfs(next, visited | (1u128 << e), mark));
        }
        best
    }

    /// Re-award the longest road card after a road or settlement placement.
    fn update_longest_road(&mut self) {
        let lens: Vec<u8> = (0..self.num_players)
            .map(|p| self.longest_road_len(p))
            .collect();
        let max = *lens.iter().max().unwrap();
        let leaders: Vec<usize> = (0..self.num_players).filter(|&p| lens[p] == max).collect();
        let unique_leader = (leaders.len() == 1 && max >= 5).then(|| leaders[0] as u8);

        self.longest_road = match self.longest_road {
            Some(h) if lens[h as usize] >= 5 => {
                // The holder keeps the card until someone strictly exceeds them.
                if max > lens[h as usize] {
                    unique_leader.or(Some(h))
                } else {
                    Some(h)
                }
            }
            // Holder fell below 5 (their road was cut): award to a unique
            // leader with 5+, otherwise no one holds it.
            _ => unique_leader,
        };
    }

    fn update_largest_army(&mut self) {
        let count = self.players[self.turn_player].knights_played;
        if count < 3 {
            return;
        }
        match self.largest_army {
            None => self.largest_army = Some(self.turn_player as u8),
            Some(h) if self.players[h as usize].knights_played < count => {
                self.largest_army = Some(self.turn_player as u8)
            }
            _ => {}
        }
    }

    /// Distribute resources for a non-7 roll, honoring the bank-shortage
    /// rule: if the bank cannot fully pay a resource and more than one player
    /// is owed it, no one receives that resource.
    fn payout_roll(&mut self, sum: u8) {
        let geo = geometry();
        let mut demand = [[0u8; 5]; MAX_PLAYERS];
        for h in 0..NUM_HEXES {
            if geo.hex_number[h] != sum || self.robber as usize == h {
                continue;
            }
            let Terrain::Producing(r) = geo.hex_terrain[h] else {
                continue;
            };
            for &v in &geo.hex_vertices[h] {
                if let Some((owner, is_city)) = self.vertex_owner(v as usize) {
                    demand[owner][r as usize] += if is_city { 2 } else { 1 };
                }
            }
        }
        for r in 0..5 {
            let total: u8 = (0..self.num_players).map(|p| demand[p][r]).sum();
            if total == 0 {
                continue;
            }
            let claimants = (0..self.num_players).filter(|&p| demand[p][r] > 0).count();
            if total <= self.bank[r] {
                for p in 0..self.num_players {
                    self.players[p].resources[r] += demand[p][r];
                }
                self.bank[r] -= total;
            } else if claimants == 1 {
                let p = (0..self.num_players).find(|&p| demand[p][r] > 0).unwrap();
                let amount = demand[p][r].min(self.bank[r]);
                self.players[p].resources[r] += amount;
                self.bank[r] -= amount;
            }
        }
    }

    fn resolve_roll(&mut self) {
        let sum = self.die1 + self.die2;
        if sum == 7 {
            for i in 0..self.num_players {
                let p = (self.turn_player + i) % self.num_players;
                let hand = self.players[p].hand_total();
                self.pending_discards[p] = if hand > 7 { hand / 2 } else { 0 };
            }
            if !self.advance_discard() {
                self.phase = CatanPhase::MoveRobber;
                self.cur = self.turn_player;
            }
        } else {
            self.payout_roll(sum);
            self.phase = CatanPhase::Main;
            self.cur = self.turn_player;
        }
    }

    /// Point `cur` at the next player who still must discard. Returns false
    /// when discarding is finished.
    fn advance_discard(&mut self) -> bool {
        for i in 0..self.num_players {
            let p = (self.turn_player + i) % self.num_players;
            if self.pending_discards[p] > 0 {
                self.phase = CatanPhase::Discard;
                self.cur = p;
                return true;
            }
        }
        false
    }

    fn place_robber(&mut self, hex: u8) {
        self.robber = hex;
        let victims = self.robber_victims();
        match victims.len() {
            0 => {
                self.phase = CatanPhase::Main;
                self.cur = self.turn_player;
            }
            1 => {
                self.steal_victim = victims[0] as u8;
                self.phase = CatanPhase::StealCard;
            }
            _ => self.phase = CatanPhase::StealChoice,
        }
    }

    fn end_turn(&mut self) {
        let p = &mut self.players[self.turn_player];
        for i in 0..5 {
            p.dev[i] += p.dev_new[i];
            p.dev_new[i] = 0;
        }
        self.dev_played_this_turn = false;
        self.turn_player = (self.turn_player + 1) % self.num_players;
        self.cur = self.turn_player;
        if self.rolls >= MAX_ROLLS {
            self.game_over = true;
        } else {
            self.phase = CatanPhase::Roll1;
        }
    }

    fn advance_setup(&mut self) {
        self.setup_idx += 1;
        let n = self.num_players;
        if self.setup_idx as usize == 2 * n {
            self.turn_player = 0;
            self.cur = 0;
            self.phase = CatanPhase::Roll1;
        } else {
            let i = self.setup_idx as usize;
            self.turn_player = if i < n { i } else { 2 * n - 1 - i };
            self.cur = self.turn_player;
            self.phase = CatanPhase::SetupSettlement;
        }
    }

    fn in_second_setup_round(&self) -> bool {
        self.setup_idx as usize >= self.num_players
    }

    fn apply_main_action(&mut self, id: u8) {
        let tp = self.turn_player;
        match id {
            END_TURN => self.end_turn(),
            BUY_DEV => {
                self.pay(tp, &DEV_COST);
                self.phase = CatanPhase::DevDraw;
            }
            PLAY_KNIGHT => {
                self.players[tp].dev[DEV_KNIGHT] -= 1;
                self.players[tp].knights_played += 1;
                self.dev_played_this_turn = true;
                self.update_largest_army();
                self.phase = CatanPhase::MoveRobber;
            }
            PLAY_ROAD_BUILDING => {
                self.players[tp].dev[DEV_ROAD_BUILDING] -= 1;
                self.dev_played_this_turn = true;
                self.free_roads = 2.min(self.players[tp].roads_left);
                self.phase = CatanPhase::FreeRoad;
            }
            PLAY_YEAR_OF_PLENTY => {
                self.players[tp].dev[DEV_YEAR_OF_PLENTY] -= 1;
                self.dev_played_this_turn = true;
                self.yop_picks = 2;
                self.phase = CatanPhase::PickYearOfPlenty;
            }
            PLAY_MONOPOLY => {
                self.players[tp].dev[DEV_MONOPOLY] -= 1;
                self.dev_played_this_turn = true;
                self.phase = CatanPhase::PickMonopoly;
            }
            _ if (TRADE_BASE..MOVE_ROBBER_BASE).contains(&id) => {
                let CatanAction::BankTrade { give, get } = CatanAction::from_action(Action(id))
                else {
                    unreachable!()
                };
                let rate = self.trade_rate(tp, give);
                self.players[tp].resources[give as usize] -= rate;
                self.bank[give as usize] += rate;
                self.bank[get as usize] -= 1;
                self.players[tp].resources[get as usize] += 1;
            }
            _ if (SETTLEMENT_BASE..CITY_BASE).contains(&id) => {
                let v = (id - SETTLEMENT_BASE) as usize;
                self.pay(tp, &SETTLEMENT_COST);
                self.vertices[v] = Self::mark(tp);
                self.players[tp].settlements_left -= 1;
                // A new settlement can cut an opponent's road.
                self.update_longest_road();
            }
            _ if (CITY_BASE..ROAD_BASE).contains(&id) => {
                let v = (id - CITY_BASE) as usize;
                self.pay(tp, &CITY_COST);
                self.vertices[v] = Self::mark(tp) + 4;
                self.players[tp].cities_left -= 1;
                self.players[tp].settlements_left += 1;
            }
            _ if (ROAD_BASE..MAX_ACTION).contains(&id) => {
                let e = (id - ROAD_BASE) as usize;
                self.pay(tp, &ROAD_COST);
                self.edges[e] = Self::mark(tp);
                self.players[tp].roads_left -= 1;
                self.update_longest_road();
            }
            _ => panic!("illegal main action {id}"),
        }
    }

    fn legal_main_actions(&self, actions: &mut Vec<Action>) {
        let tp = self.turn_player;
        let ps = &self.players[tp];
        actions.push(Action(END_TURN));
        if self.can_afford(tp, &DEV_COST) && self.dev_deck.iter().sum::<u8>() > 0 {
            actions.push(Action(BUY_DEV));
        }
        if !self.dev_played_this_turn {
            if ps.dev[DEV_KNIGHT] > 0 {
                actions.push(Action(PLAY_KNIGHT));
            }
            if ps.dev[DEV_ROAD_BUILDING] > 0 && ps.roads_left > 0 && self.any_legal_road(tp) {
                actions.push(Action(PLAY_ROAD_BUILDING));
            }
            if ps.dev[DEV_YEAR_OF_PLENTY] > 0 && self.bank.iter().sum::<u8>() > 0 {
                actions.push(Action(PLAY_YEAR_OF_PLENTY));
            }
            if ps.dev[DEV_MONOPOLY] > 0 {
                actions.push(Action(PLAY_MONOPOLY));
            }
        }
        for give in RESOURCES {
            let rate = self.trade_rate(tp, give);
            if ps.resources[give as usize] < rate {
                continue;
            }
            for get in RESOURCES {
                if give != get && self.bank[get as usize] > 0 {
                    actions.push(CatanAction::BankTrade { give, get }.into());
                }
            }
        }
        if ps.settlements_left > 0 && self.can_afford(tp, &SETTLEMENT_COST) {
            for v in 0..NUM_VERTICES {
                if self.can_place_settlement(v, tp, false) {
                    actions.push(Action(SETTLEMENT_BASE + v as u8));
                }
            }
        }
        if ps.cities_left > 0 && self.can_afford(tp, &CITY_COST) {
            for v in 0..NUM_VERTICES {
                if self.vertices[v] == Self::mark(tp) {
                    actions.push(Action(CITY_BASE + v as u8));
                }
            }
        }
        if ps.roads_left > 0 && self.can_afford(tp, &ROAD_COST) {
            for e in 0..NUM_EDGES {
                if self.can_place_road(e, tp) {
                    actions.push(Action(ROAD_BASE + e as u8));
                }
            }
        }
    }

    /// Check a single action's legality without enumerating all actions.
    /// Used by `resample_from_istate` to validate replays cheaply-ish.
    fn action_is_legal(&self, a: Action) -> bool {
        let mut legal = Vec::new();
        self.legal_actions(&mut legal);
        legal.contains(&a)
    }

    /// Hash of the full game state excluding the action history, so that
    /// transpositions reached by different paths share a key.
    fn state_hash(&self) -> u64 {
        let mut h = DefaultHasher::default();
        self.num_players.hash(&mut h);
        self.phase.hash(&mut h);
        self.turn_player.hash(&mut h);
        self.cur.hash(&mut h);
        self.players.hash(&mut h);
        self.vertices.hash(&mut h);
        self.edges.hash(&mut h);
        self.robber.hash(&mut h);
        self.bank.hash(&mut h);
        self.dev_deck.hash(&mut h);
        self.longest_road.hash(&mut h);
        self.largest_army.hash(&mut h);
        self.dev_played_this_turn.hash(&mut h);
        self.die1.hash(&mut h);
        self.die2.hash(&mut h);
        self.rolls.hash(&mut h);
        self.setup_idx.hash(&mut h);
        self.pending_discards.hash(&mut h);
        self.steal_victim.hash(&mut h);
        self.free_roads.hash(&mut h);
        self.yop_picks.hash(&mut h);
        self.game_over.hash(&mut h);
        h.finish()
    }

    fn board_hash(&self) -> u64 {
        let mut h = DefaultHasher::default();
        self.vertices.hash(&mut h);
        self.edges.hash(&mut h);
        h.finish()
    }
}

/// Read-only accessors for UIs and tooling. Everything here is derivable
/// from the action history; the game engine itself only uses the private
/// fields directly.
impl CatanGameState {
    pub fn phase(&self) -> CatanPhase {
        self.phase
    }

    /// Whose turn it is (distinct from `cur_player` while discarding).
    pub fn turn(&self) -> Player {
        self.turn_player
    }

    pub fn resources(&self, p: Player) -> [u8; 5] {
        self.players[p].resources
    }

    pub fn hand_size(&self, p: Player) -> u8 {
        self.players[p].hand_total()
    }

    /// Playable dev cards by type (excludes cards bought this turn).
    pub fn dev_playable(&self, p: Player) -> [u8; 5] {
        self.players[p].dev
    }

    pub fn dev_bought_this_turn(&self, p: Player) -> [u8; 5] {
        self.players[p].dev_new
    }

    pub fn dev_count(&self, p: Player) -> u8 {
        self.players[p].dev_total()
    }

    pub fn knights_played(&self, p: Player) -> u8 {
        self.players[p].knights_played
    }

    /// Remaining (roads, settlements, cities) in the player's supply.
    pub fn pieces_left(&self, p: Player) -> (u8, u8, u8) {
        let ps = &self.players[p];
        (ps.roads_left, ps.settlements_left, ps.cities_left)
    }

    /// (owner, is_city) of the building on a vertex, if any.
    pub fn building_at(&self, v: usize) -> Option<(Player, bool)> {
        self.vertex_owner(v)
    }

    pub fn road_at(&self, e: usize) -> Option<Player> {
        match self.edges[e] {
            0 => None,
            x => Some((x - 1) as Player),
        }
    }

    pub fn robber_hex(&self) -> u8 {
        self.robber
    }

    pub fn bank(&self) -> [u8; 5] {
        self.bank
    }

    pub fn dev_deck_len(&self) -> u8 {
        self.dev_deck.iter().sum()
    }

    /// The current dice (0 = not yet rolled this turn for that die).
    pub fn dice(&self) -> (u8, u8) {
        (self.die1, self.die2)
    }

    pub fn num_rolls(&self) -> u16 {
        self.rolls
    }

    /// Full victory points including unplayed VP dev cards. Only show a
    /// player their own full count; use `public_victory_points` for others.
    pub fn victory_points(&self, p: Player) -> u8 {
        self.vp(p)
    }

    pub fn public_victory_points(&self, p: Player) -> u8 {
        self.public_vp(p)
    }

    pub fn longest_road_holder(&self) -> Option<Player> {
        self.longest_road.map(|p| p as Player)
    }

    pub fn largest_army_holder(&self) -> Option<Player> {
        self.largest_army.map(|p| p as Player)
    }

    pub fn longest_road_length(&self, p: Player) -> u8 {
        self.longest_road_len(p)
    }

    /// Cards this player still must discard for the current 7-roll.
    pub fn pending_discard(&self, p: Player) -> u8 {
        self.pending_discards[p]
    }

    /// Who is being robbed (meaningful in the `StealCard` phase).
    pub fn steal_target(&self) -> Player {
        self.steal_victim as Player
    }

    /// The player's best bank trade rate for giving a resource (4, or 3/2
    /// with a port).
    pub fn bank_trade_rate(&self, p: Player, give: Resource) -> u8 {
        self.trade_rate(p, give)
    }

    pub fn has_played_dev_this_turn(&self) -> bool {
        self.dev_played_this_turn
    }
}

impl GameState for CatanGameState {
    fn apply_action(&mut self, a: Action) {
        debug_assert!(
            self.action_is_legal(a),
            "illegal action {} in phase {:?}\n{}",
            a,
            self.phase,
            self
        );
        self.history.push(a);
        let id = a.0;
        match self.phase {
            CatanPhase::SetupSettlement => {
                let v = (id - SETTLEMENT_BASE) as usize;
                self.vertices[v] = Self::mark(self.cur);
                self.players[self.cur].settlements_left -= 1;
                self.last_settlement = v as u8;
                if self.in_second_setup_round() {
                    // The second settlement grants one resource per adjacent
                    // producing hex.
                    let geo = geometry();
                    for &h in &geo.vertex_hexes[v] {
                        if let Terrain::Producing(r) = geo.hex_terrain[h as usize] {
                            self.bank[r as usize] -= 1;
                            self.players[self.cur].resources[r as usize] += 1;
                        }
                    }
                }
                self.phase = CatanPhase::SetupRoad;
            }
            CatanPhase::SetupRoad => {
                let e = (id - ROAD_BASE) as usize;
                self.edges[e] = Self::mark(self.cur);
                self.players[self.cur].roads_left -= 1;
                self.advance_setup();
            }
            CatanPhase::Roll1 => {
                self.die1 = id - DIE_ROLL_BASE + 1;
                self.phase = CatanPhase::Roll2;
            }
            CatanPhase::Roll2 => {
                self.die2 = id - DIE_ROLL_BASE + 1;
                self.rolls += 1;
                self.resolve_roll();
            }
            CatanPhase::Discard => {
                let r = (id - PICK_RESOURCE_BASE) as usize;
                self.players[self.cur].resources[r] -= 1;
                self.bank[r] += 1;
                self.pending_discards[self.cur] -= 1;
                if !self.advance_discard() {
                    self.phase = CatanPhase::MoveRobber;
                    self.cur = self.turn_player;
                }
            }
            CatanPhase::MoveRobber => {
                self.place_robber(id - MOVE_ROBBER_BASE);
            }
            CatanPhase::StealChoice => {
                self.steal_victim = id - STEAL_FROM_BASE;
                self.phase = CatanPhase::StealCard;
            }
            CatanPhase::StealCard => {
                let victim = self.steal_victim as usize;
                let r = slot_to_type(&self.players[victim].resources, id);
                self.players[victim].resources[r] -= 1;
                self.players[self.turn_player].resources[r] += 1;
                self.phase = CatanPhase::Main;
                self.cur = self.turn_player;
            }
            CatanPhase::DevDraw => {
                let t = slot_to_type(&self.dev_deck, id);
                self.dev_deck[t] -= 1;
                self.players[self.turn_player].dev_new[t] += 1;
                self.phase = CatanPhase::Main;
            }
            CatanPhase::Main => self.apply_main_action(id),
            CatanPhase::FreeRoad => {
                let e = (id - ROAD_BASE) as usize;
                let tp = self.turn_player;
                self.edges[e] = Self::mark(tp);
                self.players[tp].roads_left -= 1;
                self.update_longest_road();
                self.free_roads -= 1;
                if self.free_roads == 0
                    || self.players[tp].roads_left == 0
                    || !self.any_legal_road(tp)
                {
                    self.free_roads = 0;
                    self.phase = CatanPhase::Main;
                }
            }
            CatanPhase::PickYearOfPlenty => {
                let r = (id - PICK_RESOURCE_BASE) as usize;
                self.bank[r] -= 1;
                self.players[self.turn_player].resources[r] += 1;
                self.yop_picks -= 1;
                if self.yop_picks == 0 || self.bank.iter().sum::<u8>() == 0 {
                    self.yop_picks = 0;
                    self.phase = CatanPhase::Main;
                }
            }
            CatanPhase::PickMonopoly => {
                let r = (id - PICK_RESOURCE_BASE) as usize;
                let tp = self.turn_player;
                for p in 0..self.num_players {
                    if p == tp {
                        continue;
                    }
                    let taken = self.players[p].resources[r];
                    self.players[p].resources[r] = 0;
                    self.players[tp].resources[r] += taken;
                }
                self.phase = CatanPhase::Main;
            }
        }

        if !self.game_over && self.vp(self.turn_player) >= WIN_VP {
            self.game_over = true;
        }
    }

    fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();
        if self.is_terminal() {
            return;
        }
        match self.phase {
            CatanPhase::SetupSettlement => {
                for v in 0..NUM_VERTICES {
                    if self.can_place_settlement(v, self.cur, true) {
                        actions.push(Action(SETTLEMENT_BASE + v as u8));
                    }
                }
            }
            CatanPhase::SetupRoad => {
                let geo = geometry();
                for &e in &geo.vertex_edges[self.last_settlement as usize] {
                    if self.edges[e as usize] == 0 {
                        actions.push(Action(ROAD_BASE + e));
                    }
                }
            }
            CatanPhase::Roll1 | CatanPhase::Roll2 => {
                for v in 0..6 {
                    actions.push(Action(DIE_ROLL_BASE + v));
                }
            }
            CatanPhase::Discard => {
                for r in 0..5 {
                    if self.players[self.cur].resources[r] > 0 {
                        actions.push(Action(PICK_RESOURCE_BASE + r as u8));
                    }
                }
            }
            CatanPhase::MoveRobber => {
                for h in 0..NUM_HEXES as u8 {
                    if h != self.robber {
                        actions.push(Action(MOVE_ROBBER_BASE + h));
                    }
                }
            }
            CatanPhase::StealChoice => {
                for p in self.robber_victims() {
                    actions.push(Action(STEAL_FROM_BASE + p as u8));
                }
            }
            CatanPhase::StealCard => {
                let total = self.players[self.steal_victim as usize].hand_total();
                for s in 0..total {
                    actions.push(Action(s));
                }
            }
            CatanPhase::DevDraw => {
                let total: u8 = self.dev_deck.iter().sum();
                for s in 0..total {
                    actions.push(Action(s));
                }
            }
            CatanPhase::Main => self.legal_main_actions(actions),
            CatanPhase::FreeRoad => {
                for e in 0..NUM_EDGES {
                    if self.can_place_road(e, self.turn_player) {
                        actions.push(Action(ROAD_BASE + e as u8));
                    }
                }
            }
            CatanPhase::PickYearOfPlenty => {
                for r in 0..5 {
                    if self.bank[r] > 0 {
                        actions.push(Action(PICK_RESOURCE_BASE + r as u8));
                    }
                }
            }
            CatanPhase::PickMonopoly => {
                for r in 0..5 {
                    actions.push(Action(PICK_RESOURCE_BASE + r as u8));
                }
            }
        }
    }

    /// Score: victory points scaled to ~1, plus a bonus of 1 for the winner.
    /// At the roll cap the (unique) VP leader is the winner; a tie means no
    /// winner bonus.
    fn evaluate(&self, p: Player) -> f64 {
        if !self.is_terminal() {
            panic!("evaluate called on non-terminal gamestate");
        }
        let vps: Vec<u8> = (0..self.num_players).map(|q| self.vp(q)).collect();
        let winner = if let Some(w) = (0..self.num_players).find(|&q| vps[q] >= WIN_VP) {
            Some(w)
        } else {
            let max = *vps.iter().max().unwrap();
            let leaders: Vec<usize> =
                (0..self.num_players).filter(|&q| vps[q] == max).collect();
            (leaders.len() == 1).then(|| leaders[0])
        };
        vps[p] as f64 / WIN_VP as f64 + if winner == Some(p) { 1.0 } else { 0.0 }
    }

    /// A compact observation key, NOT a perfect-recall action history: full
    /// games run hundreds of actions but `IStateKey` holds at most 64. All
    /// fields are observable by `player`; the public board is folded into an
    /// 8-byte hash.
    fn istate_key(&self, player: Player) -> IStateKey {
        let mut key = IStateKey::default();
        key.push(Action(self.phase as u8));
        key.push(Action(self.turn_player as u8));
        key.push(Action(self.cur as u8));
        key.push(Action(player as u8));
        let ps = &self.players[player];
        for r in 0..5 {
            key.push(Action(ps.resources[r]));
        }
        for d in 0..5 {
            key.push(Action(ps.dev[d]));
        }
        for d in 0..5 {
            key.push(Action(ps.dev_new[d]));
        }
        key.push(Action(self.robber));
        key.push(Action(self.die1));
        key.push(Action(self.die2));
        key.push(Action((self.rolls & 0xff) as u8));
        key.push(Action((self.rolls >> 8) as u8));
        key.push(Action(self.free_roads));
        key.push(Action(self.yop_picks));
        key.push(Action(if matches!(self.phase, CatanPhase::StealCard) {
            self.steal_victim + 1
        } else {
            0
        }));
        for p in 0..self.num_players {
            key.push(Action(self.pending_discards[p]));
        }
        key.push(Action(self.longest_road.map_or(0, |p| p + 1)));
        key.push(Action(self.largest_army.map_or(0, |p| p + 1)));
        for p in 0..self.num_players {
            key.push(Action(self.players[p].hand_total()));
            key.push(Action(self.players[p].dev_total()));
            key.push(Action(self.players[p].knights_played));
            key.push(Action(self.public_vp(p)));
        }
        for b in self.board_hash().to_le_bytes() {
            key.push(Action(b));
        }
        key
    }

    fn istate_string(&self, player: Player) -> String {
        let ps = &self.players[player];
        let res: String = RESOURCES
            .iter()
            .map(|&r| format!("{}{}", r.char(), ps.resources[r as usize]))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "P{}|{:?}|turn:P{}|roll:{}{}|res:{}|dev:{:?}+{:?}|vp:{}|board:{:016x}",
            player,
            self.phase,
            self.turn_player,
            self.die1,
            self.die2,
            res,
            ps.dev,
            ps.dev_new,
            self.vp(player),
            self.board_hash(),
        )
    }

    fn is_terminal(&self) -> bool {
        self.game_over
    }

    fn is_chance_node(&self) -> bool {
        matches!(
            self.phase,
            CatanPhase::Roll1 | CatanPhase::Roll2 | CatanPhase::StealCard | CatanPhase::DevDraw
        )
    }

    fn num_players(&self) -> usize {
        self.num_players
    }

    fn cur_player(&self) -> Player {
        self.cur
    }

    /// Like `istate_key`, this is a hashed full-state key rather than an
    /// action history (which would not fit in an `IStateKey`).
    fn key(&self) -> IStateKey {
        let mut key = IStateKey::default();
        key.push(Action(self.phase as u8));
        key.push(Action(self.turn_player as u8));
        key.push(Action(self.cur as u8));
        for b in self.state_hash().to_le_bytes() {
            key.push(Action(b));
        }
        key
    }

    fn undo(&mut self) {
        // State is a pure function of the history: pop and replay.
        let mut history = std::mem::take(&mut self.history);
        history
            .pop()
            .expect("undo called on the initial gamestate");
        let mut ngs = Catan::new_state(self.num_players);
        for a in history {
            ngs.apply_action(a);
        }
        *self = ngs;
    }
}

impl Display for CatanGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Catan {}p phase:{:?} turn:P{} cur:P{} roll:{}+{} rolls:{} robber:{}",
            self.num_players,
            self.phase,
            self.turn_player,
            self.cur,
            self.die1,
            self.die2,
            self.rolls,
            self.robber
        )?;
        for p in 0..self.num_players {
            let ps = &self.players[p];
            writeln!(
                f,
                "  P{p}: vp:{} res:{:?} dev:{:?}+{:?} knights:{} pieces r/s/c:{}/{}/{}",
                self.vp(p),
                ps.resources,
                ps.dev,
                ps.dev_new,
                ps.knights_played,
                ps.roads_left,
                ps.settlements_left,
                ps.cities_left,
            )?;
        }
        let settlements: Vec<String> = (0..NUM_VERTICES)
            .filter_map(|v| {
                self.vertex_owner(v).map(|(p, city)| {
                    format!("{}{}@{}", if city { "C" } else { "S" }, p, v)
                })
            })
            .collect();
        let roads: Vec<String> = (0..NUM_EDGES)
            .filter(|&e| self.edges[e] != 0)
            .map(|e| format!("R{}@{}", self.edges[e] - 1, e))
            .collect();
        write!(
            f,
            "  buildings: {} | roads: {} | bank:{:?} deck:{:?}",
            settlements.join(" "),
            roads.join(" "),
            self.bank,
            self.dev_deck
        )
    }
}

/// A hidden chance outcome in the true history, used to resample worlds.
#[derive(Clone, Copy)]
enum ChanceEvent {
    /// A dev card draw: (buyer, card type drawn).
    Dev(Player, usize),
    /// A robber steal: (thief, victim, resource stolen).
    Steal(Player, Player, usize),
}

impl ResampleFromInfoState for CatanGameState {
    /// The only hidden information (no player-to-player trades in v1) is the
    /// identity of dev cards drawn by other players and of cards stolen in
    /// robberies the observer was not part of. Re-deal those chance outcomes
    /// uniformly and replay the public history; rejection-sample until the
    /// replay is consistent (e.g. an opponent who later played a knight must
    /// have drawn one), falling back to the true state if no consistent
    /// sample is found.
    fn resample_from_istate<T: rand::Rng>(&self, player: Player, rng: &mut T) -> Self {
        use rand::RngExt;
        // Resolve what each hidden chance action actually selected.
        let mut events: Vec<Option<ChanceEvent>> = Vec::with_capacity(self.history.len());
        let mut sim = Catan::new_state(self.num_players);
        for &a in &self.history {
            events.push(match sim.phase {
                CatanPhase::DevDraw => Some(ChanceEvent::Dev(
                    sim.turn_player,
                    slot_to_type(&sim.dev_deck, a.0),
                )),
                CatanPhase::StealCard => Some(ChanceEvent::Steal(
                    sim.turn_player,
                    sim.steal_victim as usize,
                    slot_to_type(&sim.players[sim.steal_victim as usize].resources, a.0),
                )),
                _ => None,
            });
            sim.apply_action(a);
        }

        let target = self.istate_key(player);
        'attempt: for _ in 0..50 {
            let mut ngs = Catan::new_state(self.num_players);
            for (i, &a) in self.history.iter().enumerate() {
                let chosen = match events[i] {
                    Some(ChanceEvent::Dev(buyer, card)) => {
                        if buyer == player {
                            // The observer knows their own card: redraw the
                            // same type from the resampled deck.
                            match first_slot_of_type(&ngs.dev_deck, card) {
                                Some(s) => Action(s),
                                None => continue 'attempt,
                            }
                        } else {
                            let total: u8 = ngs.dev_deck.iter().sum();
                            Action(rng.random_range(0..total))
                        }
                    }
                    Some(ChanceEvent::Steal(thief, victim, resource)) => {
                        if thief == player || victim == player {
                            match first_slot_of_type(&ngs.players[victim].resources, resource) {
                                Some(s) => Action(s),
                                None => continue 'attempt,
                            }
                        } else {
                            let total = ngs.players[victim].hand_total();
                            Action(rng.random_range(0..total))
                        }
                    }
                    None => a,
                };
                // Hidden hands may have diverged enough to make a later
                // public action illegal in this world; resample.
                if !ngs.action_is_legal(chosen) {
                    continue 'attempt;
                }
                ngs.apply_action(chosen);
            }
            if ngs.istate_key(player) == target {
                return ngs;
            }
        }
        // No consistent sample found: fall back to the true world.
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

    use super::*;
    use crate::actions;

    fn random_playthrough(num_players: usize, seed: u64, max_steps: usize) -> CatanGameState {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut gs = Catan::new_state(num_players);
        let mut legal = Vec::new();
        for _ in 0..max_steps {
            if gs.is_terminal() {
                break;
            }
            gs.legal_actions(&mut legal);
            assert!(!legal.is_empty(), "no legal actions in {:?}\n{}", gs.phase, gs);
            let mut sorted = legal.clone();
            sorted.sort();
            assert_eq!(legal, sorted, "actions not sorted in {:?}", gs.phase);
            let a = *legal.choose(&mut rng).unwrap();
            gs.apply_action(a);
        }
        gs
    }

    fn assert_conservation(gs: &CatanGameState) {
        // 19 of each resource split between the bank and hands.
        for r in 0..5 {
            let total: u8 =
                gs.bank[r] + (0..gs.num_players).map(|p| gs.players[p].resources[r]).sum::<u8>();
            assert_eq!(total, 19, "resource {r} not conserved");
        }
        // 25 dev cards split between the deck and hands (played cards are
        // removed from hands but tracked for knights).
        let in_hands: u8 = (0..gs.num_players).map(|p| gs.players[p].dev_total()).sum();
        let played: u8 = (0..gs.num_players)
            .map(|p| gs.players[p].knights_played)
            .sum();
        assert!(gs.dev_deck.iter().sum::<u8>() + in_hands + played <= 25);
    }

    #[test]
    fn setup_phase_places_two_settlements_and_roads_each() {
        for n in [2, 3, 4] {
            let gs = random_playthrough_to_main(n, 42);
            for p in 0..n {
                assert_eq!(gs.players[p].settlements_left, 3);
                assert_eq!(gs.players[p].roads_left, 13);
                // The second settlement granted 1-3 resources.
                let hand = gs.players[p].hand_total();
                assert!((1..=3).contains(&hand), "P{p} setup hand {hand}");
            }
            assert_conservation(&gs);
        }
    }

    /// Random-play through setup, stopping right when setup ends.
    fn random_playthrough_to_main(num_players: usize, seed: u64) -> CatanGameState {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut gs = Catan::new_state(num_players);
        let mut legal = Vec::new();
        while matches!(gs.phase, CatanPhase::SetupSettlement | CatanPhase::SetupRoad) {
            gs.legal_actions(&mut legal);
            let a = *legal.choose(&mut rng).unwrap();
            gs.apply_action(a);
        }
        assert_eq!(gs.phase, CatanPhase::Roll1);
        gs
    }

    #[test]
    fn random_games_terminate_with_invariants() {
        for n in [2, 3, 4] {
            for seed in 0..5 {
                let gs = random_playthrough(n, seed, 100_000);
                assert!(gs.is_terminal(), "game did not terminate");
                assert_conservation(&gs);
                let mut legal = Vec::new();
                gs.legal_actions(&mut legal);
                assert!(legal.is_empty());
                // evaluate runs and produces sane scores
                for p in 0..n {
                    let v = gs.evaluate(p);
                    assert!((0.0..=3.0).contains(&v), "score {v}");
                }
            }
        }
    }

    #[test]
    fn undo_is_inverse_of_apply() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(7);
        let mut legal = Vec::new();
        for seed in 0..2 {
            let mut gs = Catan::new_state(4);
            let mut steps = 0;
            while !gs.is_terminal() && steps < 600 {
                gs.legal_actions(&mut legal);
                let a = *legal.choose(&mut rng).unwrap();
                let before = gs.clone();
                gs.apply_action(a);
                gs.undo();
                assert_eq!(gs, before, "undo mismatch at step {steps} seed {seed}");
                gs.apply_action(a);
                steps += 1;
            }
        }
    }

    #[test]
    fn distance_rule_blocks_adjacent_settlement() {
        let mut gs = Catan::new_state(2);
        let legal = actions!(gs);
        // All 54 vertices open initially.
        assert_eq!(legal.len(), NUM_VERTICES);
        let first = legal[0];
        gs.apply_action(first);
        // Place P0's road, then P1 must avoid vertex 0 and its neighbors.
        let road = actions!(gs)[0];
        gs.apply_action(road);
        let v0 = (first.0 - SETTLEMENT_BASE) as usize;
        let p1_options = actions!(gs);
        assert!(!p1_options.contains(&first));
        for &w in &geometry().vertex_neighbors[v0] {
            assert!(!p1_options.contains(&Action(SETTLEMENT_BASE + w)));
        }
    }

    #[test]
    fn longest_road_simple_chain_and_cut() {
        let geo = geometry();
        let mut gs = Catan::new_state(2);
        // Build a 6-road simple path for P0 by walking unvisited neighbors.
        let mut path = vec![0u8];
        for _ in 0..6 {
            let v = *path.last().unwrap();
            let &next = geo.vertex_neighbors[v as usize]
                .iter()
                .find(|w| !path.contains(w))
                .unwrap();
            let e = geo.vertex_edges[v as usize]
                .iter()
                .find(|&&e| {
                    let (a, b) = geo.edge_vertices[e as usize];
                    (a == v && b == next) || (a == next && b == v)
                })
                .copied()
                .unwrap();
            gs.edges[e as usize] = 1;
            path.push(next);
        }
        assert_eq!(gs.longest_road_len(0), 6);
        gs.update_longest_road();
        assert_eq!(gs.longest_road, Some(0));

        // An opponent settlement in the middle of the chain cuts it 3/3.
        gs.vertices[path[3] as usize] = 2; // P1 settlement
        assert_eq!(gs.longest_road_len(0), 3);
        gs.update_longest_road();
        assert_eq!(gs.longest_road, None, "cut road below 5 loses the card");
    }

    #[test]
    fn trade_rates_respect_ports() {
        let geo = geometry();
        let mut gs = Catan::new_state(2);
        assert_eq!(gs.trade_rate(0, Resource::Brick), 4);

        let three_to_one = (0..NUM_VERTICES)
            .find(|&v| geo.vertex_port[v] == Some(Port::ThreeToOne))
            .unwrap();
        gs.vertices[three_to_one] = 1;
        assert_eq!(gs.trade_rate(0, Resource::Brick), 3);

        let brick_port = (0..NUM_VERTICES)
            .find(|&v| geo.vertex_port[v] == Some(Port::TwoToOne(Resource::Brick)))
            .unwrap();
        gs.vertices[brick_port] = 1;
        assert_eq!(gs.trade_rate(0, Resource::Brick), 2);
        assert_eq!(gs.trade_rate(0, Resource::Wool), 3);
        assert_eq!(gs.trade_rate(1, Resource::Brick), 4);
    }

    #[test]
    fn bank_shortage_rule() {
        let mut gs = Catan::new_state(2);
        let geo = geometry();
        // Find a producing hex and put settlements of both players on it.
        let hex = (0..NUM_HEXES)
            .find(|&h| matches!(geo.hex_terrain[h], Terrain::Producing(_)))
            .unwrap();
        let Terrain::Producing(r) = geo.hex_terrain[hex] else {
            unreachable!()
        };
        gs.vertices[geo.hex_vertices[hex][0] as usize] = 1; // P0 settlement
        gs.vertices[geo.hex_vertices[hex][3] as usize] = 2; // P1 settlement
        gs.bank[r as usize] = 1; // Not enough for both.
        gs.payout_roll(geo.hex_number[hex]);
        assert_eq!(gs.players[0].resources[r as usize], 0);
        assert_eq!(gs.players[1].resources[r as usize], 0);
        assert_eq!(gs.bank[r as usize], 1);

        // With a single claimant they get what is left.
        gs.vertices[geo.hex_vertices[hex][3] as usize] = 0;
        gs.payout_roll(geo.hex_number[hex]);
        assert_eq!(gs.players[0].resources[r as usize], 1);
        assert_eq!(gs.bank[r as usize], 0);
    }

    #[test]
    fn monopoly_collects_all_of_resource() {
        let mut gs = Catan::new_state(3);
        gs.phase = CatanPhase::PickMonopoly;
        gs.turn_player = 0;
        gs.cur = 0;
        gs.players[1].resources[Resource::Grain as usize] = 4;
        gs.players[2].resources[Resource::Grain as usize] = 2;
        gs.bank[Resource::Grain as usize] = 13;
        gs.apply_action(CatanAction::PickResource(Resource::Grain).into());
        assert_eq!(gs.players[0].resources[Resource::Grain as usize], 6);
        assert_eq!(gs.players[1].resources[Resource::Grain as usize], 0);
        assert_eq!(gs.players[2].resources[Resource::Grain as usize], 0);
        assert_eq!(gs.phase, CatanPhase::Main);
    }

    #[test]
    fn knight_play_moves_robber_and_steals() {
        let geo = geometry();
        let mut gs = Catan::new_state(2);
        gs.phase = CatanPhase::Main;
        gs.players[0].dev[DEV_KNIGHT] = 3;
        gs.players[1].resources[Resource::Ore as usize] = 1;
        // P1 settlement on hex 0.
        gs.vertices[geo.hex_vertices[0][0] as usize] = 2;

        gs.apply_action(CatanAction::PlayKnight.into());
        assert_eq!(gs.phase, CatanPhase::MoveRobber);
        gs.apply_action(CatanAction::MoveRobber(0).into());
        // Single victim with cards: steal chance node.
        assert_eq!(gs.phase, CatanPhase::StealCard);
        assert!(gs.is_chance_node());
        let slots = actions!(gs);
        assert_eq!(slots.len(), 1);
        gs.apply_action(slots[0]);
        assert_eq!(gs.players[0].resources[Resource::Ore as usize], 1);
        assert_eq!(gs.players[1].resources[Resource::Ore as usize], 0);
        assert_eq!(gs.phase, CatanPhase::Main);
        assert_eq!(gs.players[0].knights_played, 1);
        // One dev card per turn.
        assert!(!actions!(gs).contains(&CatanAction::PlayKnight.into()));
    }

    #[test]
    fn largest_army_awarded_at_three_knights() {
        let mut gs = Catan::new_state(2);
        gs.players[0].knights_played = 2;
        gs.turn_player = 0;
        gs.players[0].knights_played += 1;
        gs.update_largest_army();
        assert_eq!(gs.largest_army, Some(0));
        assert_eq!(gs.public_vp(0), 2);

        // P1 must exceed, not tie.
        gs.turn_player = 1;
        gs.players[1].knights_played = 3;
        gs.update_largest_army();
        assert_eq!(gs.largest_army, Some(0));
        gs.players[1].knights_played = 4;
        gs.update_largest_army();
        assert_eq!(gs.largest_army, Some(1));
    }

    #[test]
    fn seven_triggers_discards_over_seven_cards() {
        let mut gs = random_playthrough_to_main(2, 3);
        gs.players[0].resources = [9, 0, 0, 0, 0];
        // Force a 7: apply die rolls of 3 and 4.
        gs.apply_action(CatanAction::DieRoll(3).into());
        gs.apply_action(CatanAction::DieRoll(4).into());
        assert_eq!(gs.phase, CatanPhase::Discard);
        assert_eq!(gs.pending_discards[0], 4);
        for _ in 0..4 {
            assert_eq!(gs.cur_player(), 0);
            gs.apply_action(CatanAction::PickResource(Resource::Brick).into());
        }
        assert_eq!(gs.phase, CatanPhase::MoveRobber);
        assert_eq!(gs.players[0].resources[0], 5);
    }

    #[test]
    fn resample_preserves_observer_istate() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(11);
        for n in [2, 3, 4] {
            for seed in 0..3 {
                for steps in [50, 200, 500] {
                    let gs = random_playthrough(n, seed, steps);
                    if gs.is_terminal() {
                        continue;
                    }
                    for p in 0..n {
                        let resampled = gs.resample_from_istate(p, &mut rng);
                        assert_eq!(
                            resampled.istate_key(p),
                            gs.istate_key(p),
                            "resample changed P{p}'s istate (n={n} seed={seed} steps={steps})"
                        );
                        // Public board state must match exactly.
                        assert_eq!(resampled.vertices, gs.vertices);
                        assert_eq!(resampled.edges, gs.edges);
                        assert_eq!(resampled.robber, gs.robber);
                        assert_eq!(resampled.phase, gs.phase);
                        assert_eq!(resampled.cur, gs.cur);
                    }
                }
            }
        }
    }

}
