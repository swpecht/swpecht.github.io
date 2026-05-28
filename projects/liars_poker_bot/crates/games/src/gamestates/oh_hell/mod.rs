//! Oh Hell (a.k.a. Oh Pshaw, Bust, Blackout). 3-player full-deck variant.
//!
//! Reference: <https://en.wikipedia.org/wiki/Oh_hell>
//!
//! Rules implemented here:
//! - 3 players, standard 52-card deck.
//! - `n_tricks` cards dealt to each player, one card flipped as trump.
//!   Bound: `MAX_TRICKS = 10` (limited by the 64-action `IStateKey`).
//! - Player 0 (eldest hand) bids first, then 1, then 2. Bids are public
//!   integers in [0, n_tricks].
//! - Player 0 leads the first trick; trick winner leads the next.
//! - Must follow lead suit if possible; otherwise may play any card.
//!   Highest trump beats; if no trump played, highest of lead suit wins.
//! - Scoring: a player who takes exactly their bid scores `10 + bid`;
//!   anyone who misses scores 0. `evaluate(p)` returns p's score minus the
//!   mean of all players' scores so the game is zero-sum.

use std::fmt::{Display, Write};

use serde::{Deserialize, Serialize};

use crate::{
    istate::IStateKey,
    resample::ResampleFromInfoState,
    {Action, Game, GameState, Player},
};

use self::actions::{OHAction, OHCard, OHSuit, OH_DECK, OH_DECK_SIZE};

pub mod actions;

pub const NUM_PLAYERS: usize = 3;

/// Maximum tricks per hand supported. Bounded by the 64-action IStateKey:
/// total actions per hand = 3 * n_tricks (deal) + 1 (face up) + 3 (bids) +
/// 3 * n_tricks (play) = 6 * n_tricks + 4. Solving for ≤ 64 gives 10.
pub const MAX_TRICKS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum OHPhase {
    #[default]
    DealHands,
    DealFaceUp,
    Bidding,
    Play,
    Terminal,
}

pub struct OhHell {}

impl OhHell {
    pub fn new_state(n_tricks: usize) -> OhHellGameState {
        assert!(n_tricks >= 1, "n_tricks must be at least 1");
        assert!(
            n_tricks <= MAX_TRICKS,
            "n_tricks {} exceeds MAX_TRICKS {}",
            n_tricks,
            MAX_TRICKS
        );
        assert!(
            NUM_PLAYERS * n_tricks + 1 <= OH_DECK_SIZE,
            "deck too small for {} tricks * {} players",
            n_tricks,
            NUM_PLAYERS
        );

        OhHellGameState {
            n_tricks: n_tricks as u8,
            cur_player: 0,
            phase: OHPhase::DealHands,
            hands: [0; NUM_PLAYERS],
            face_up: None,
            trump_suit: None,
            bids: [None; NUM_PLAYERS],
            num_bids: 0,
            trick_cards: [None; NUM_PLAYERS],
            num_in_trick: 0,
            trick_starter: 0,
            trick_winners: Vec::new(),
            tricks_won: [0; NUM_PLAYERS],
            cards_played: 0,
            key: IStateKey::default(),
            play_order: Vec::new(),
        }
    }

    pub fn game(n_tricks: usize) -> Game<OhHellGameState> {
        // Compile-time-friendly closure for each supported n_tricks.
        let new_f: fn() -> OhHellGameState = match n_tricks {
            1 => || OhHell::new_state(1),
            2 => || OhHell::new_state(2),
            3 => || OhHell::new_state(3),
            4 => || OhHell::new_state(4),
            5 => || OhHell::new_state(5),
            6 => || OhHell::new_state(6),
            7 => || OhHell::new_state(7),
            8 => || OhHell::new_state(8),
            9 => || OhHell::new_state(9),
            10 => || OhHell::new_state(10),
            _ => panic!("unsupported n_tricks: {}", n_tricks),
        };
        Game {
            new: Box::new(new_f),
            max_players: NUM_PLAYERS,
            // 52 cards + (n_tricks + 1) bid options. Generous upper bound.
            max_actions: OH_DECK_SIZE + n_tricks + 1,
        }
    }

    /// Construct a state from a sequence of actions. Useful for tests.
    pub fn from_actions(n_tricks: usize, actions: &[OHAction]) -> OhHellGameState {
        let mut gs = OhHell::new_state(n_tricks);
        for &a in actions {
            gs.apply_action(a.into());
        }
        gs
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OhHellGameState {
    n_tricks: u8,
    cur_player: Player,
    phase: OHPhase,

    /// Bitmask of cards held in each player's hand, indexed by `OHCard as u8`.
    /// 52-bit mask packed into u64 (bits 0..52 used).
    hands: [u64; NUM_PLAYERS],
    face_up: Option<OHCard>,
    trump_suit: Option<OHSuit>,

    bids: [Option<u8>; NUM_PLAYERS],
    num_bids: u8,

    /// Cards in the trick currently in progress (slot per position in the
    /// trick — 0 = trick starter, 1 = next, 2 = last).
    trick_cards: [Option<OHCard>; NUM_PLAYERS],
    num_in_trick: u8,
    trick_starter: Player,

    /// Winner of each completed trick, in order.
    trick_winners: Vec<Player>,
    tricks_won: [u8; NUM_PLAYERS],
    cards_played: u8,

    /// Full action history (every action ever taken). Used for undo and
    /// to construct per-player istate keys.
    key: IStateKey,
    /// Player who took each action in `key`. play_order[i] is the actor for
    /// key[i]. Length always matches `key`.
    play_order: Vec<Player>,
}

impl OhHellGameState {
    pub fn n_tricks(&self) -> usize {
        self.n_tricks as usize
    }

    pub fn phase(&self) -> OHPhase {
        self.phase
    }

    pub fn trump_suit(&self) -> Option<OHSuit> {
        self.trump_suit
    }

    pub fn face_up(&self) -> Option<OHCard> {
        self.face_up
    }

    pub fn bids(&self) -> [Option<u8>; NUM_PLAYERS] {
        self.bids
    }

    pub fn tricks_won(&self) -> [u8; NUM_PLAYERS] {
        self.tricks_won
    }

    pub fn cards_played(&self) -> usize {
        self.cards_played as usize
    }

    pub fn get_hand(&self, player: Player) -> Vec<OHCard> {
        cards_from_mask(self.hands[player])
    }

    fn deal_card(&mut self, player: Player, card: OHCard) {
        let bit = 1u64 << (card as u8);
        debug_assert!(
            self.hands[player] & bit == 0,
            "card already in player's hand"
        );
        self.hands[player] |= bit;
    }

    fn remove_card(&mut self, player: Player, card: OHCard) {
        let bit = 1u64 << (card as u8);
        debug_assert!(
            self.hands[player] & bit != 0,
            "card not in player's hand"
        );
        self.hands[player] &= !bit;
    }

    /// All cards that have not been placed anywhere yet (still in the
    /// undealt deck, not in any player's hand, not the face-up card, and
    /// not yet played).
    fn undealt_cards(&self) -> Vec<OHCard> {
        let mut used: u64 = self.hands.iter().fold(0u64, |a, b| a | b);
        if let Some(c) = self.face_up {
            used |= 1u64 << (c as u8);
        }
        for a in self.key.iter() {
            if let OHAction::Card(c) = OHAction::from(*a) {
                used |= 1u64 << (c as u8);
            }
        }

        OH_DECK
            .iter()
            .copied()
            .filter(|c| used & (1u64 << (*c as u8)) == 0)
            .collect()
    }

    fn legal_actions_deal_hands(&self, actions: &mut Vec<Action>) {
        for c in self.undealt_cards() {
            actions.push(OHAction::Card(c).into());
        }
    }

    fn legal_actions_deal_face_up(&self, actions: &mut Vec<Action>) {
        for c in self.undealt_cards() {
            actions.push(OHAction::Card(c).into());
        }
    }

    fn legal_actions_bidding(&self, actions: &mut Vec<Action>) {
        for n in 0..=self.n_tricks {
            actions.push(OHAction::Bid(n).into());
        }
    }

    fn legal_actions_play(&self, actions: &mut Vec<Action>) {
        let hand_mask = self.hands[self.cur_player];
        if self.num_in_trick == 0 {
            push_hand(hand_mask, actions);
            return;
        }
        let lead_card = self.trick_cards[0].expect("first slot filled when num_in_trick>0");
        let lead_suit = lead_card.suit();
        let lead_mask = mask_for_suit(lead_suit) & hand_mask;
        if lead_mask != 0 {
            push_hand(lead_mask, actions);
        } else {
            push_hand(hand_mask, actions);
        }
    }

    fn apply_deal_hands(&mut self, card: OHCard) {
        let p = self.cur_player;
        self.deal_card(p, card);

        let dealt_so_far = self.key.len() + 1;
        if dealt_so_far == NUM_PLAYERS * self.n_tricks as usize {
            self.phase = OHPhase::DealFaceUp;
            self.cur_player = 0;
        } else {
            self.cur_player = (p + 1) % NUM_PLAYERS;
        }
    }

    fn apply_deal_face_up(&mut self, card: OHCard) {
        self.face_up = Some(card);
        self.trump_suit = Some(card.suit());
        self.phase = OHPhase::Bidding;
        self.cur_player = 0;
    }

    fn apply_bid(&mut self, n: u8) {
        debug_assert!(n <= self.n_tricks, "bid out of range");
        let p = self.cur_player;
        debug_assert!(self.bids[p].is_none());
        self.bids[p] = Some(n);
        self.num_bids += 1;

        if self.num_bids as usize == NUM_PLAYERS {
            self.phase = OHPhase::Play;
            self.cur_player = 0;
            self.trick_starter = 0;
            self.num_in_trick = 0;
        } else {
            self.cur_player = (p + 1) % NUM_PLAYERS;
        }
    }

    fn apply_play(&mut self, card: OHCard) {
        let p = self.cur_player;
        self.remove_card(p, card);
        self.trick_cards[self.num_in_trick as usize] = Some(card);
        self.num_in_trick += 1;
        self.cards_played += 1;

        if self.num_in_trick as usize == NUM_PLAYERS {
            let winner = self.trick_winner();
            self.trick_winners.push(winner);
            self.tricks_won[winner] += 1;
            self.trick_cards = [None; NUM_PLAYERS];
            self.num_in_trick = 0;
            self.trick_starter = winner;
            self.cur_player = winner;

            if self.cards_played as usize == NUM_PLAYERS * self.n_tricks as usize {
                self.phase = OHPhase::Terminal;
            }
        } else {
            self.cur_player = (p + 1) % NUM_PLAYERS;
        }
    }

    /// Determine the winner of the just-completed trick.
    fn trick_winner(&self) -> Player {
        let trump = self.trump_suit.expect("trump must be set in play phase");
        let lead = self.trick_cards[0].expect("trick has plays").suit();

        let mut best_pos = 0usize;
        let mut best_card = self.trick_cards[0].unwrap();
        for i in 1..NUM_PLAYERS {
            let c = self.trick_cards[i].expect("trick fully played");
            if beats(c, best_card, lead, trump) {
                best_card = c;
                best_pos = i;
            }
        }
        (self.trick_starter + best_pos) % NUM_PLAYERS
    }

    /// Build the score vector. Each player who made their bid exactly scores
    /// `10 + bid`; everyone else gets 0.
    fn raw_scores(&self) -> [f64; NUM_PLAYERS] {
        let mut out = [0.0; NUM_PLAYERS];
        for p in 0..NUM_PLAYERS {
            let bid = self.bids[p].expect("bids set when evaluating");
            if self.tricks_won[p] == bid {
                out[p] = 10.0 + bid as f64;
            }
        }
        out
    }
}

fn mask_for_suit(suit: OHSuit) -> u64 {
    let mut m = 0u64;
    for &c in &OH_DECK {
        if c.suit() == suit {
            m |= 1u64 << (c as u8);
        }
    }
    m
}

/// Bit flag for a suit (used in `forbidden_suits` masks).
fn suit_bit(suit: OHSuit) -> u8 {
    1u8 << (suit as u8)
}

/// Returns true if `candidate` beats `current_best` given lead suit and trump.
fn beats(candidate: OHCard, current_best: OHCard, lead: OHSuit, trump: OHSuit) -> bool {
    let c_trump = candidate.suit() == trump;
    let b_trump = current_best.suit() == trump;
    match (c_trump, b_trump) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => candidate.rank() > current_best.rank(),
        (false, false) => {
            let c_lead = candidate.suit() == lead;
            let b_lead = current_best.suit() == lead;
            match (c_lead, b_lead) {
                (true, false) => true,
                (false, true) => false,
                (false, false) => false,
                (true, true) => candidate.rank() > current_best.rank(),
            }
        }
    }
}

fn push_hand(hand_mask: u64, actions: &mut Vec<Action>) {
    let mut mask = hand_mask;
    while mask != 0 {
        let bit = mask.trailing_zeros() as u8;
        mask &= mask - 1;
        actions.push(Action(bit));
    }
}

fn cards_from_mask(mask: u64) -> Vec<OHCard> {
    let mut out = Vec::new();
    let mut m = mask;
    while m != 0 {
        let bit = m.trailing_zeros() as u8;
        m &= m - 1;
        out.push(OHCard::from_index(bit).expect("valid card bit"));
    }
    out
}

impl GameState for OhHellGameState {
    fn apply_action(&mut self, a: Action) {
        self.play_order.push(self.cur_player);
        let oa = OHAction::from(a);
        match (self.phase, oa) {
            (OHPhase::DealHands, OHAction::Card(c)) => self.apply_deal_hands(c),
            (OHPhase::DealFaceUp, OHAction::Card(c)) => self.apply_deal_face_up(c),
            (OHPhase::Bidding, OHAction::Bid(n)) => self.apply_bid(n),
            (OHPhase::Play, OHAction::Card(c)) => self.apply_play(c),
            (phase, action) => panic!(
                "invalid action {:?} for phase {:?} (state: {})",
                action, phase, self
            ),
        }
        self.key.push(a);
    }

    fn legal_actions(&self, actions: &mut Vec<Action>) {
        actions.clear();
        match self.phase {
            OHPhase::DealHands => self.legal_actions_deal_hands(actions),
            OHPhase::DealFaceUp => self.legal_actions_deal_face_up(actions),
            OHPhase::Bidding => self.legal_actions_bidding(actions),
            OHPhase::Play => self.legal_actions_play(actions),
            OHPhase::Terminal => {}
        }
    }

    fn evaluate(&self, p: Player) -> f64 {
        assert!(self.is_terminal(), "evaluate called on non-terminal");
        let scores = self.raw_scores();
        let mean: f64 = scores.iter().sum::<f64>() / NUM_PLAYERS as f64;
        scores[p] - mean
    }

    fn istate_key(&self, player: Player) -> IStateKey {
        let mut k = IStateKey::default();
        let deal_count = NUM_PLAYERS * self.n_tricks as usize;
        for (i, (p, a)) in self.play_order.iter().zip(self.key.iter()).enumerate() {
            let visible = if i < deal_count {
                *p == player
            } else {
                true
            };
            if visible {
                k.push(*a);
            }
        }
        let n_hand = (self.n_tricks as usize).min(k.len());
        k.sort_range(0, n_hand);
        k
    }

    fn istate_string(&self, player: Player) -> String {
        let k = self.istate_key(player);
        let mut s = String::new();
        let n_hand = self.n_tricks as usize;

        for i in 0..n_hand.min(k.len()) {
            let a = OHAction::from(k[i]);
            write!(s, "{}", a).unwrap();
        }

        if k.len() <= n_hand {
            return s;
        }

        // Face up card
        s.push('|');
        let face_up_idx = n_hand;
        write!(s, "{}", OHAction::from(k[face_up_idx])).unwrap();

        // Bids
        let bids_start = face_up_idx + 1;
        let bids_end = (bids_start + NUM_PLAYERS).min(k.len());
        if bids_end > bids_start {
            s.push('|');
            for i in bids_start..bids_end {
                write!(s, "{}", OHAction::from(k[i])).unwrap();
            }
        }

        // Plays, broken into tricks of NUM_PLAYERS cards
        let plays_start = bids_start + NUM_PLAYERS;
        if k.len() > plays_start {
            let mut i = plays_start;
            while i < k.len() {
                if (i - plays_start) % NUM_PLAYERS == 0 {
                    s.push('|');
                }
                write!(s, "{}", OHAction::from(k[i])).unwrap();
                i += 1;
            }
        }

        s
    }

    fn is_terminal(&self) -> bool {
        self.phase == OHPhase::Terminal
    }

    fn is_chance_node(&self) -> bool {
        matches!(self.phase, OHPhase::DealHands | OHPhase::DealFaceUp)
    }

    fn num_players(&self) -> usize {
        NUM_PLAYERS
    }

    fn cur_player(&self) -> Player {
        self.cur_player
    }

    fn key(&self) -> IStateKey {
        // Canonicalize deal order: sort each player's deal segment so isomorphic
        // states share a key.
        let mut sorted = self.key;
        let deal_count = NUM_PLAYERS * self.n_tricks as usize;
        if sorted.len() >= NUM_PLAYERS {
            for p in 0..NUM_PLAYERS {
                let mut cards: Vec<Action> = (0..self.n_tricks as usize)
                    .map(|t| t * NUM_PLAYERS + p)
                    .filter(|i| *i < deal_count.min(sorted.len()))
                    .map(|i| sorted[i])
                    .collect();
                cards.sort();
                for (t, a) in cards.iter().enumerate() {
                    let idx = t * NUM_PLAYERS + p;
                    if idx < sorted.len() {
                        sorted[idx] = *a;
                    }
                }
            }
        }
        sorted
    }

    fn undo(&mut self) {
        let last_player = self.play_order.pop().expect("non-empty play_order");
        let last_action = self.key.pop();
        let oa = OHAction::from(last_action);

        let n_after = self.key.len();
        let deal_count = NUM_PLAYERS * self.n_tricks as usize;

        if n_after < deal_count {
            // Was a hand-deal action.
            if let OHAction::Card(c) = oa {
                self.remove_card(last_player, c);
            }
            self.phase = OHPhase::DealHands;
            self.cur_player = last_player;
        } else if n_after == deal_count {
            // Was the face-up action.
            self.face_up = None;
            self.trump_suit = None;
            self.phase = OHPhase::DealFaceUp;
            self.cur_player = last_player;
        } else if n_after < deal_count + 1 + NUM_PLAYERS {
            // Was a bidding action.
            if let OHAction::Bid(_) = oa {
                self.bids[last_player] = None;
                self.num_bids -= 1;
            }
            self.phase = OHPhase::Bidding;
            self.cur_player = last_player;
        } else {
            // Was a play action.
            let OHAction::Card(card) = oa else {
                panic!("expected Card during play undo");
            };
            self.phase = OHPhase::Play;

            if self.num_in_trick == 0 {
                // We just popped a trick-closing card. Undo the trick winner.
                let winner = self.trick_winners.pop().expect("trick winners non-empty");
                self.tricks_won[winner] -= 1;

                let plays_remaining = NUM_PLAYERS - 1;
                self.num_in_trick = plays_remaining as u8;
                self.trick_cards = [None; NUM_PLAYERS];
                let start = self.key.len() - plays_remaining;
                for i in 0..plays_remaining {
                    let a = self.key[start + i];
                    let OHAction::Card(c) = OHAction::from(a) else {
                        panic!("expected card in play history");
                    };
                    self.trick_cards[i] = Some(c);
                }
                self.trick_starter = self.play_order[start];

                self.deal_card(last_player, card);
                self.cur_player = last_player;
                self.cards_played -= 1;
            } else {
                self.num_in_trick -= 1;
                self.trick_cards[self.num_in_trick as usize] = None;
                self.deal_card(last_player, card);
                self.cur_player = last_player;
                self.cards_played -= 1;
            }
        }
    }
}

impl Display for OhHellGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // [Hands]|FaceUp|Bids|Trick1|Trick2|...
        let deal_count = NUM_PLAYERS * self.n_tricks as usize;
        let n_hand = self.n_tricks as usize;

        for p in 0..NUM_PLAYERS {
            if p > 0 {
                f.write_char('|')?;
            }
            let mut cards: Vec<OHCard> = (0..n_hand)
                .map(|t| t * NUM_PLAYERS + p)
                .filter(|i| *i < self.key.len().min(deal_count))
                .filter_map(|i| match OHAction::from(self.key[i]) {
                    OHAction::Card(c) => Some(c),
                    _ => None,
                })
                .collect();
            cards.sort();
            for c in cards {
                write!(f, "{}", c)?;
            }
        }

        if self.key.len() <= deal_count {
            return Ok(());
        }

        f.write_char('|')?;
        if let OHAction::Card(c) = OHAction::from(self.key[deal_count]) {
            write!(f, "{}", c)?;
        }

        let bids_start = deal_count + 1;
        let bids_end = (bids_start + NUM_PLAYERS).min(self.key.len());
        if bids_end > bids_start {
            f.write_char('|')?;
            for i in bids_start..bids_end {
                write!(f, "{}", OHAction::from(self.key[i]))?;
            }
        }

        let plays_start = bids_start + NUM_PLAYERS;
        let mut i = plays_start;
        while i < self.key.len() {
            if (i - plays_start) % NUM_PLAYERS == 0 {
                f.write_char('|')?;
            }
            write!(f, "{}", OHAction::from(self.key[i]))?;
            i += 1;
        }
        Ok(())
    }
}

// ============================================================================
// Resample (constraint-respecting via backtracking)
// ============================================================================

impl ResampleFromInfoState for OhHellGameState {
    /// Resample a state that produces the same istate for `player`.
    ///
    /// Approach:
    /// 1. Gather public/observed info: face-up card; `player`'s full initial
    ///    hand; every card each other player has visibly played; suit
    ///    restrictions (a player who failed to follow a suit has none of it).
    /// 2. Build the unknown pool (cards not revealed by step 1).
    /// 3. Run a backtracking solver to assign pool cards to the other
    ///    players' hidden hand slots while respecting per-suit constraints.
    ///    Unused pool cards represent the undealt stock.
    /// 4. Replay the entire action history against a fresh state, swapping
    ///    in the resampled cards for the other players' deal actions.
    fn resample_from_istate<T: rand::Rng>(&self, player: Player, rng: &mut T) -> Self {
        let n_tricks = self.n_tricks as usize;
        let deal_count = NUM_PLAYERS * n_tricks;

        // ---- 1. Gather public info ----

        let player_initial_cards: Vec<OHCard> = (0..n_tricks)
            .map(|t| t * NUM_PLAYERS + player)
            .filter(|i| *i < self.key.len().min(deal_count))
            .filter_map(|i| match OHAction::from(self.key[i]) {
                OHAction::Card(c) => Some(c),
                _ => None,
            })
            .collect();
        let mut player_initial_mask: u64 = 0;
        for c in &player_initial_cards {
            player_initial_mask |= 1u64 << (*c as u8);
        }

        let face_up = if self.key.len() > deal_count {
            match OHAction::from(self.key[deal_count]) {
                OHAction::Card(c) => Some(c),
                _ => None,
            }
        } else {
            None
        };
        let face_up_mask: u64 = face_up.map(|c| 1u64 << (c as u8)).unwrap_or(0);

        // Per-player publicly-played cards and forbidden-suits inferred from
        // follow-suit failures.
        let mut played_mask = [0u64; NUM_PLAYERS];
        let plays_start = deal_count + 1 + NUM_PLAYERS;
        let mut trick_lead_suit: Option<OHSuit> = None;
        let mut trick_pos: usize = 0;
        let mut forbidden_suits: [u8; NUM_PLAYERS] = [0; NUM_PLAYERS];

        for i in plays_start..self.key.len() {
            let actor = self.play_order[i];
            let OHAction::Card(c) = OHAction::from(self.key[i]) else {
                continue;
            };
            played_mask[actor] |= 1u64 << (c as u8);

            if trick_pos == 0 {
                trick_lead_suit = Some(c.suit());
            } else if let Some(lead) = trick_lead_suit {
                if c.suit() != lead {
                    forbidden_suits[actor] |= suit_bit(lead);
                }
            }
            trick_pos += 1;
            if trick_pos == NUM_PLAYERS {
                trick_pos = 0;
                trick_lead_suit = None;
            }
        }

        // ---- 2. Build the unknown pool and per-player budgets ----

        let played_count_p = |p: Player| -> usize { played_mask[p].count_ones() as usize };

        let mut budgets: [usize; NUM_PLAYERS] = [0; NUM_PLAYERS];
        for q in 0..NUM_PLAYERS {
            if q != player {
                budgets[q] = n_tricks.saturating_sub(played_count_p(q));
            }
        }

        let revealed: u64 = player_initial_mask
            | face_up_mask
            | played_mask.iter().fold(0u64, |a, b| a | b);
        let unknown_pool: Vec<OHCard> = OH_DECK
            .iter()
            .copied()
            .filter(|c| revealed & (1u64 << (*c as u8)) == 0)
            .collect();

        let total_budget: usize = budgets.iter().sum();
        debug_assert!(
            total_budget <= unknown_pool.len(),
            "constraint inference failure: need {} cards, pool has {}",
            total_budget,
            unknown_pool.len()
        );

        // ---- 3. Constraint-propagating backtracking solver ----

        // Fail-fast per-suit feasibility check (necessary condition for any
        // assignment to exist). If the public history is consistent this
        // always passes; treat a failure here as a bug.
        debug_assert!(
            per_suit_feasibility(
                &unknown_pool,
                &budgets,
                &forbidden_suits,
                total_budget,
                player,
            ),
            "constraint inference produced an infeasible per-suit assignment"
        );

        // Reorder pool so cards in the most-constrained suit come first
        // (fewer eligible players ⇒ smaller branching factor). Within a
        // suit, shuffle for randomness. This is a constraint-propagation
        // heuristic that drastically prunes the search tree.
        let pool_order = build_constrained_pool_order(
            &unknown_pool,
            &forbidden_suits,
            player,
            rng,
        );

        let mut assignment: [Vec<OHCard>; NUM_PLAYERS] = Default::default();

        let success = solve_assignment(
            &mut assignment,
            &budgets,
            &forbidden_suits,
            &pool_order,
            0,
            total_budget,
            player,
        );
        assert!(
            success,
            "backtracking solver failed for player {} on state {}",
            player, self
        );

        // ---- 4. Replay ----

        // For each other player, deal their already-played cards FIRST (so
        // subsequent play actions remain legal), then their assigned hidden
        // cards. For `player`, reuse the exact initial deal in original order.
        let mut deal_pool: [Vec<OHCard>; NUM_PLAYERS] = Default::default();
        for q in 0..NUM_PLAYERS {
            if q == player {
                deal_pool[q] = player_initial_cards.clone();
            } else {
                let mut played: Vec<OHCard> = OH_DECK
                    .iter()
                    .copied()
                    .filter(|c| played_mask[q] & (1u64 << (*c as u8)) != 0)
                    .collect();
                played.extend(assignment[q].iter().copied());
                deal_pool[q] = played;
            }
        }

        let mut ngs = OhHell::new_state(n_tricks);

        for i in 0..self.key.len() {
            let orig_actor = self.play_order[i];
            if i < deal_count {
                let pool = &mut deal_pool[orig_actor];
                let c = pool.remove(0);
                ngs.apply_action(OHAction::Card(c).into());
            } else {
                ngs.apply_action(self.key[i]);
            }
        }

        debug_assert_eq!(
            ngs.istate_key(player),
            self.istate_key(player),
            "resample produced inconsistent istate"
        );
        ngs
    }
}

/// Per-suit feasibility check (a necessary condition for any valid
/// assignment to exist). For each suit, the number of pool cards in that
/// suit must fit into the slots-eligible-for-that-suit plus the available
/// stock capacity. Catches inference contradictions before we even begin
/// backtracking.
fn per_suit_feasibility(
    pool: &[OHCard],
    budgets: &[usize; NUM_PLAYERS],
    forbidden: &[u8; NUM_PLAYERS],
    needed: usize,
    skip_player: Player,
) -> bool {
    let stock_slots = pool.len().saturating_sub(needed);
    for suit in OHSuit::ALL {
        let bit = suit_bit(suit);
        let cards_in_suit = pool.iter().filter(|c| c.suit() == suit).count();
        let mut eligible_slots: usize = 0;
        for p in 0..NUM_PLAYERS {
            if p == skip_player {
                continue;
            }
            if forbidden[p] & bit == 0 {
                eligible_slots += budgets[p];
            }
        }
        if cards_in_suit > eligible_slots + stock_slots {
            return false;
        }
    }
    true
}

/// Order the pool so the most-constrained cards (those eligible for the
/// fewest players) appear first. This is a constraint-propagation
/// heuristic: deciding constrained variables first prunes huge subtrees.
/// Within a constraint tier, randomize for unbiased resampling.
fn build_constrained_pool_order<T: rand::Rng>(
    pool: &[OHCard],
    forbidden: &[u8; NUM_PLAYERS],
    skip_player: Player,
    rng: &mut T,
) -> Vec<OHCard> {
    use rand::seq::SliceRandom;

    // Eligible-player count per suit.
    let mut eligible_count = [0u8; 4];
    for (i, suit) in OHSuit::ALL.iter().enumerate() {
        let bit = suit_bit(*suit);
        for p in 0..NUM_PLAYERS {
            if p == skip_player {
                continue;
            }
            if forbidden[p] & bit == 0 {
                eligible_count[i] += 1;
            }
        }
    }

    // Group cards by suit, then concatenate groups in ascending eligibility
    // order. Each group is shuffled internally for randomness.
    let mut groups: [Vec<OHCard>; 4] = Default::default();
    for &c in pool {
        groups[c.suit() as usize].push(c);
    }
    for g in groups.iter_mut() {
        g.shuffle(rng);
    }

    let mut suit_order: Vec<usize> = (0..4).collect();
    suit_order.sort_by_key(|&i| eligible_count[i]);

    let mut out = Vec::with_capacity(pool.len());
    for s in suit_order {
        out.extend(groups[s].iter().copied());
    }
    out
}

/// Backtracking solver with forward-checking constraint propagation.
///
/// At each step the solver:
///   1. Detects success/failure via budget accounting.
///   2. Runs per-suit forward checks against the *remaining* pool. If any
///      suit can no longer satisfy its demand from eligible slots + stock,
///      the branch is pruned without descending.
///   3. Tries assigning the current pool card to each eligible player,
///      then tries placing it in the stock.
fn solve_assignment(
    assignment: &mut [Vec<OHCard>; NUM_PLAYERS],
    budgets: &[usize; NUM_PLAYERS],
    forbidden: &[u8; NUM_PLAYERS],
    pool: &[OHCard],
    pool_pos: usize,
    needed: usize,
    skip_player: Player,
) -> bool {
    if needed == 0 {
        return true;
    }
    let remaining = pool.len() - pool_pos;
    if remaining < needed {
        return false;
    }

    // Forward check: for the remaining pool slice, ensure no suit can
    // become infeasible. The "eligible budget" for each suit is the sum of
    // *remaining* per-player capacity (budget - already-assigned) for
    // players that can hold this suit.
    let stock_remaining = remaining - needed;
    let pool_tail = &pool[pool_pos..];
    for suit in OHSuit::ALL {
        let bit = suit_bit(suit);
        let cards_in_suit = pool_tail.iter().filter(|c| c.suit() == suit).count();
        if cards_in_suit == 0 {
            continue;
        }
        let mut eligible_slots: usize = 0;
        for p in 0..NUM_PLAYERS {
            if p == skip_player {
                continue;
            }
            if forbidden[p] & bit != 0 {
                continue;
            }
            eligible_slots += budgets[p] - assignment[p].len();
        }
        if cards_in_suit > eligible_slots + stock_remaining {
            return false;
        }
    }

    let card = pool[pool_pos];
    let bit = suit_bit(card.suit());

    // Try assigning to each eligible player.
    for p in 0..NUM_PLAYERS {
        if p == skip_player {
            continue;
        }
        if assignment[p].len() >= budgets[p] {
            continue;
        }
        if forbidden[p] & bit != 0 {
            continue;
        }
        assignment[p].push(card);
        if solve_assignment(
            assignment,
            budgets,
            forbidden,
            pool,
            pool_pos + 1,
            needed - 1,
            skip_player,
        ) {
            return true;
        }
        assignment[p].pop();
    }

    // Skip this card (leaves it in stock). Only feasible if the remaining
    // pool *without* this card can still cover the budget.
    if remaining - 1 >= needed
        && solve_assignment(
            assignment,
            budgets,
            forbidden,
            pool,
            pool_pos + 1,
            needed,
            skip_player,
        )
    {
        return true;
    }

    false
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::actions::{OHAction, OHCard, OHSuit};
    use super::*;
    use crate::{actions, GameState};
    use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

    // ---------------- Action / Phase basics ----------------

    #[test]
    fn fresh_state_in_dealing_phase() {
        let gs = OhHell::new_state(2);
        assert_eq!(gs.phase(), OHPhase::DealHands);
        assert!(gs.is_chance_node());
        assert!(!gs.is_terminal());
        assert_eq!(gs.num_players(), NUM_PLAYERS);
    }

    #[test]
    fn dealing_legal_actions_are_remaining_cards() {
        let gs = OhHell::new_state(2);
        let a = actions!(gs);
        assert_eq!(a.len(), OH_DECK_SIZE, "fresh deck should have 52 cards");
    }

    #[test]
    fn dealing_does_not_repeat_cards() {
        let mut gs = OhHell::new_state(2);
        let cards = [
            OHCard::NS, OHCard::TS, OHCard::JS, OHCard::QS, OHCard::KS, OHCard::NC,
        ];
        for c in cards {
            let a = actions!(gs);
            assert!(a.contains(&OHAction::Card(c).into()));
            gs.apply_action(OHAction::Card(c).into());
        }
        let a = actions!(gs);
        for c in cards {
            assert!(!a.contains(&OHAction::Card(c).into()), "card {} reused", c);
        }
    }

    fn deal_and_face_up(n_tricks: usize) -> OhHellGameState {
        let mut gs = OhHell::new_state(n_tricks);
        // Deal sequentially from OH_DECK; face up takes the next one.
        let mut idx = 0;
        while gs.phase() == OHPhase::DealHands {
            gs.apply_action(OHAction::Card(OH_DECK[idx]).into());
            idx += 1;
        }
        assert_eq!(gs.phase(), OHPhase::DealFaceUp);
        gs.apply_action(OHAction::Card(OH_DECK[idx]).into());
        gs
    }

    #[test]
    fn transitions_to_bidding_after_face_up() {
        let gs = deal_and_face_up(2);
        assert_eq!(gs.phase(), OHPhase::Bidding);
        assert!(!gs.is_chance_node());
        assert_eq!(gs.cur_player(), 0);
        assert!(gs.trump_suit().is_some());
        assert!(gs.face_up().is_some());
    }

    #[test]
    fn bidding_legal_actions_are_all_bid_values() {
        let gs = deal_and_face_up(2);
        let a = actions!(gs);
        assert_eq!(a.len(), 3); // bids 0..=2
        assert_eq!(a[0], OHAction::Bid(0).into());
        assert_eq!(a[1], OHAction::Bid(1).into());
        assert_eq!(a[2], OHAction::Bid(2).into());
    }

    #[test]
    fn bidding_legal_actions_scale_with_n_tricks() {
        let gs = deal_and_face_up(5);
        let a = actions!(gs);
        assert_eq!(a.len(), 6); // bids 0..=5
    }

    #[test]
    fn after_three_bids_play_begins() {
        let mut gs = deal_and_face_up(2);
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(2).into());
        assert_eq!(gs.phase(), OHPhase::Play);
        assert_eq!(gs.cur_player(), 0);
        assert_eq!(gs.bids(), [Some(1), Some(0), Some(2)]);
    }

    // ---------------- Play / trick winner ----------------

    /// Convenience fixture. Hands (2-trick):
    ///   P0: NS, TS  (9s, Ts)
    ///   P1: JS, QS  (Js, Qs)
    ///   P2: KS, NC  (Ks, 9c)
    /// Face up: TC (trump = Clubs)
    fn fixture_clubs_trump() -> OhHellGameState {
        let mut gs = OhHell::new_state(2);
        let order = [
            OHCard::NS, OHCard::JS, OHCard::KS,
            OHCard::TS, OHCard::QS, OHCard::NC,
        ];
        for c in order {
            gs.apply_action(OHAction::Card(c).into());
        }
        gs.apply_action(OHAction::Card(OHCard::TC).into());
        assert_eq!(gs.trump_suit(), Some(OHSuit::Clubs));
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs
    }

    #[test]
    fn must_follow_suit_when_possible() {
        let mut gs = fixture_clubs_trump();
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        let legal = actions!(gs);
        assert!(legal.contains(&OHAction::Card(OHCard::JS).into()));
        assert!(legal.contains(&OHAction::Card(OHCard::QS).into()));
        assert_eq!(legal.len(), 2);
    }

    #[test]
    fn highest_trump_wins() {
        let mut gs = fixture_clubs_trump();
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::QS).into());
        let legal = actions!(gs);
        assert!(legal.contains(&OHAction::Card(OHCard::KS).into()));
        assert_eq!(legal.len(), 1);
        gs.apply_action(OHAction::Card(OHCard::KS).into());
        assert_eq!(gs.tricks_won(), [0, 0, 1]);
        assert_eq!(gs.cur_player(), 2);
    }

    #[test]
    fn no_trump_in_trick_lead_suit_wins() {
        let mut gs = fixture_clubs_trump();
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::QS).into());
        gs.apply_action(OHAction::Card(OHCard::KS).into());
        assert_eq!(gs.tricks_won(), [0, 0, 1]);
    }

    #[test]
    fn full_game_terminal_and_scoring() {
        let mut gs = fixture_clubs_trump();
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::QS).into());
        gs.apply_action(OHAction::Card(OHCard::KS).into());
        gs.apply_action(OHAction::Card(OHCard::NC).into());
        gs.apply_action(OHAction::Card(OHCard::TS).into());
        gs.apply_action(OHAction::Card(OHCard::JS).into());
        assert!(gs.is_terminal());
        assert_eq!(gs.tricks_won(), [0, 0, 2]);
        for p in 0..NUM_PLAYERS {
            assert_eq!(gs.evaluate(p), 0.0);
        }
    }

    #[test]
    fn making_your_bid_pays_off() {
        let mut gs = OhHell::new_state(1);
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::TS).into());
        gs.apply_action(OHAction::Card(OHCard::KS).into());
        gs.apply_action(OHAction::Card(OHCard::JC).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::TS).into());
        gs.apply_action(OHAction::Card(OHCard::KS).into());
        assert!(gs.is_terminal());
        let p0 = gs.evaluate(0);
        let p1 = gs.evaluate(1);
        let p2 = gs.evaluate(2);
        assert!((p0 + p1 + p2).abs() < 1e-9, "scores must be zero-sum");
        assert!(p2 > p0 && p2 > p1, "P2 should have the highest score");
    }

    // ---------------- Multi-suit play (full-deck specific) ----------------

    /// Off-suit non-trump cards do not win even if their rank is high.
    #[test]
    fn off_suit_high_card_loses() {
        // n=1 trick. P0 leads 9s, P1 plays AH (off-suit, off-trump),
        // P2 plays TS. Trump is clubs (from face-up), so AH cannot win.
        let mut gs = OhHell::new_state(1);
        gs.apply_action(OHAction::Card(OHCard::NS).into()); // P0
        gs.apply_action(OHAction::Card(OHCard::AH).into()); // P1
        gs.apply_action(OHAction::Card(OHCard::TS).into()); // P2
        gs.apply_action(OHAction::Card(OHCard::JC).into()); // face up (trump = clubs)
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Card(OHCard::NS).into()); // P0 leads 9s (spades)
        // P1 has no spades; AH is off-suit & off-trump → can play any.
        gs.apply_action(OHAction::Card(OHCard::AH).into());
        // P2 has TS (spades). Must follow spades.
        gs.apply_action(OHAction::Card(OHCard::TS).into());
        assert!(gs.is_terminal());
        // P2's TS beats P0's NS in lead suit; AH is irrelevant.
        assert_eq!(gs.tricks_won(), [0, 0, 1]);
    }

    /// A trump card always beats off-suit, even a high one.
    #[test]
    fn trump_beats_high_off_suit() {
        // n=1 trick. Face up TC (trump = clubs). P0 leads AS (lead = spades).
        // P1 plays 2C (trump). P2 plays KS (highest spade).
        // 2C should win because it's trump.
        let mut gs = OhHell::new_state(1);
        gs.apply_action(OHAction::Card(OHCard::AS).into()); // P0
        gs.apply_action(OHAction::Card(OHCard::_2C).into()); // P1
        gs.apply_action(OHAction::Card(OHCard::KS).into()); // P2
        gs.apply_action(OHAction::Card(OHCard::TC).into()); // face up
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Card(OHCard::AS).into());
        gs.apply_action(OHAction::Card(OHCard::_2C).into());
        gs.apply_action(OHAction::Card(OHCard::KS).into());
        assert!(gs.is_terminal());
        // P1's 2C trumped both spades.
        assert_eq!(gs.tricks_won(), [0, 1, 0]);
    }

    // ---------------- Terminal / Undo / Istate ----------------

    #[test]
    fn terminal_has_no_legal_actions() {
        let mut gs = fixture_clubs_trump();
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::QS).into());
        gs.apply_action(OHAction::Card(OHCard::KS).into());
        gs.apply_action(OHAction::Card(OHCard::NC).into());
        gs.apply_action(OHAction::Card(OHCard::TS).into());
        gs.apply_action(OHAction::Card(OHCard::JS).into());
        assert!(gs.is_terminal());
        let a = actions!(gs);
        assert!(a.is_empty());
    }

    #[test]
    fn undo_round_trip_random() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xCAFE);
        for _ in 0..100 {
            let mut gs = OhHell::new_state(2);
            while !gs.is_terminal() {
                let a = actions!(gs);
                assert!(!a.is_empty());
                let action = *a.choose(&mut rng).unwrap();
                let before = gs.clone();
                gs.apply_action(action);
                let mut tmp = gs.clone();
                tmp.undo();
                assert_eq!(
                    tmp, before,
                    "undo did not restore.\nbefore: {}\nafter undo: {}\nafter apply: {}",
                    before, tmp, gs
                );
            }
        }
    }

    #[test]
    fn legal_actions_always_sorted() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(7);
        for _ in 0..30 {
            let mut gs = OhHell::new_state(2);
            while !gs.is_terminal() {
                let a = actions!(gs);
                let mut sorted = a.clone();
                sorted.sort();
                assert_eq!(a, sorted, "legal_actions not sorted: {:?}", a);
                let action = *a.choose(&mut rng).unwrap();
                gs.apply_action(action);
            }
        }
    }

    #[test]
    fn istate_hides_other_players_cards() {
        let gs = deal_and_face_up(2);
        let i0 = gs.istate_key(0);
        let i1 = gs.istate_key(1);
        assert_eq!(i0.len(), 3); // 2 hand cards + face up
        assert_eq!(i1.len(), 3);
        assert_ne!(i0, i1);
        assert_eq!(i0[2], i1[2]); // face up is shared
    }

    #[test]
    fn istate_unique_per_step() {
        use std::collections::HashSet;
        let mut rng: StdRng = SeedableRng::seed_from_u64(101);
        for _ in 0..30 {
            let mut gs = OhHell::new_state(2);
            while gs.is_chance_node() {
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
            let mut seen = HashSet::new();
            seen.insert(gs.istate_string(gs.cur_player()));
            while !gs.is_terminal() {
                let a = actions!(gs);
                let action = *a.choose(&mut rng).unwrap();
                gs.apply_action(action);
                if !gs.is_terminal() {
                    let s = gs.istate_string(gs.cur_player());
                    assert!(seen.insert(s), "duplicate istate seen");
                }
            }
        }
    }

    #[test]
    fn istate_string_renders_phases() {
        let mut gs = deal_and_face_up(2);
        let s = gs.istate_string(0);
        assert!(s.contains('|'), "expected pipe in istate string: {}", s);
        gs.apply_action(OHAction::Bid(1).into());
        let s = gs.istate_string(0);
        assert!(s.matches('|').count() >= 2, "{}", s);
    }

    #[test]
    fn evaluate_is_zero_sum() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(99);
        for _ in 0..50 {
            let mut gs = OhHell::new_state(2);
            while !gs.is_terminal() {
                let a = actions!(gs);
                let action = *a.choose(&mut rng).unwrap();
                gs.apply_action(action);
            }
            let total: f64 = (0..NUM_PLAYERS).map(|p| gs.evaluate(p)).sum();
            assert!(total.abs() < 1e-9, "scores not zero-sum: {}", total);
        }
    }

    // ---------------- Resample (backtracking) ----------------

    #[test]
    fn resample_preserves_istate_at_play_boundary() {
        // After bidding, before any plays — exercises the basic dealing
        // resample with no follow-suit constraints.
        let mut rng: StdRng = SeedableRng::seed_from_u64(13);
        for _ in 0..20 {
            let mut gs = OhHell::new_state(2);
            while gs.is_chance_node() {
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
            // Through bidding.
            while gs.phase() == OHPhase::Bidding {
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
            for p in 0..NUM_PLAYERS {
                let orig = gs.istate_key(p);
                for _ in 0..10 {
                    let resampled = gs.resample_from_istate(p, &mut rng);
                    assert_eq!(resampled.istate_key(p), orig);
                }
            }
        }
    }

    #[test]
    fn resample_preserves_istate_mid_game() {
        // Walk forward through play, sampling at every decision point.
        let mut rng: StdRng = SeedableRng::seed_from_u64(31);
        for _ in 0..20 {
            let mut gs = OhHell::new_state(2);
            while gs.is_chance_node() {
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
            while !gs.is_terminal() {
                let p = gs.cur_player();
                let orig = gs.istate_key(p);
                for _ in 0..5 {
                    let resampled = gs.resample_from_istate(p, &mut rng);
                    assert_eq!(resampled.istate_key(p), orig);
                }
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
        }
    }

    /// Crafted scenario exercising the follow-suit constraint: P1 is known
    /// to have no spades because they failed to follow on the first trick.
    /// Backtracking must avoid giving P1 any spade cards.
    #[test]
    fn resample_respects_follow_suit_constraint() {
        let mut gs = OhHell::new_state(2);
        // Deal:
        //   P0: NS, TS  (spades, in order via deal positions 0, 3)
        //   P1: NH, TH  (hearts only — so cannot follow spades)
        //   P2: KS, NC  (one spade, one club)
        // Face up: TC (trump = clubs)
        let order = [
            OHCard::NS, OHCard::NH, OHCard::KS,
            OHCard::TS, OHCard::TH, OHCard::NC,
        ];
        for c in order {
            gs.apply_action(OHAction::Card(c).into());
        }
        gs.apply_action(OHAction::Card(OHCard::TC).into());
        // Bids
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(1).into());
        // P0 leads 9s. P1 has no spades → plays 9h.
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::NH).into());
        gs.apply_action(OHAction::Card(OHCard::KS).into()); // P2 follows spades.

        // From P0's perspective, P1 played 9h on a spade lead → P1 has no
        // spades. Resample 100 times and verify P1's hand never contains
        // a spade.
        let mut rng: StdRng = SeedableRng::seed_from_u64(2026);
        for _ in 0..100 {
            let r = gs.resample_from_istate(0, &mut rng);
            let p1_hand = r.get_hand(1);
            for c in p1_hand {
                assert_ne!(
                    c.suit(),
                    OHSuit::Spades,
                    "constraint violated: P1 should have no spades but holds {}",
                    c
                );
            }
        }
    }

    /// Backtracking solver succeeds for a 5-trick game (many cards, many
    /// constraints) — exercises deeper recursion.
    #[test]
    fn resample_works_for_larger_game() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(5);
        let mut gs = OhHell::new_state(5);
        while gs.is_chance_node() {
            let a = actions!(gs);
            gs.apply_action(*a.choose(&mut rng).unwrap());
        }
        while gs.phase() == OHPhase::Bidding {
            let a = actions!(gs);
            gs.apply_action(*a.choose(&mut rng).unwrap());
        }
        // Play several tricks.
        for _ in 0..6 {
            if gs.is_terminal() {
                break;
            }
            let a = actions!(gs);
            gs.apply_action(*a.choose(&mut rng).unwrap());
        }
        // Now resample for each player.
        for p in 0..NUM_PLAYERS {
            let orig = gs.istate_key(p);
            for _ in 0..5 {
                let r = gs.resample_from_istate(p, &mut rng);
                assert_eq!(r.istate_key(p), orig);
            }
        }
    }
}
