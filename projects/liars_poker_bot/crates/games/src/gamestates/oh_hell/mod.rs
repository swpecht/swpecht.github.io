//! Oh Hell (a.k.a. Oh Pshaw, Bust, Blackout). Multi-player full-deck variant.
//!
//! Reference: <https://en.wikipedia.org/wiki/Oh_hell>
//!
//! Rules implemented here:
//! - Configurable player count (2..=`MAX_PLAYERS`), standard 52-card deck.
//! - `n_tricks` cards dealt to each player, one card flipped as trump.
//!   Total IStateKey actions = `2 * num_players * n_tricks + 1 + num_players`,
//!   so the legal n_tricks range scales with the player count (≤15 for 2
//!   players, ≤10 for 3, ≤8 for 4, etc.).
//! - Player 0 (eldest hand) bids first, then 1, 2, ... in turn. Bids are
//!   public integers in [0, n_tricks].
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
pub mod processors;

/// Compile-time upper bound on the player count. Stored arrays
/// (`hands`, `bids`, `trick_cards`, `tricks_won`) are sized to this; the
/// runtime `num_players` field selects how many slots are live.
pub const MAX_PLAYERS: usize = 7;

/// Compute the maximum supported `n_tricks` for a given `num_players`.
/// Bounded by the 64-action IStateKey: total actions per hand =
/// `2 * num_players * n_tricks + num_players + 1`.
pub const fn max_tricks_for(num_players: usize) -> usize {
    // (64 - num_players - 1) / (2 * num_players), integer division.
    let budget = 64 - num_players - 1;
    budget / (2 * num_players)
}

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
    pub fn new_state(num_players: usize, n_tricks: usize) -> OhHellGameState {
        assert!(
            (2..=MAX_PLAYERS).contains(&num_players),
            "num_players {} must be in 2..={}",
            num_players,
            MAX_PLAYERS
        );
        assert!(n_tricks >= 1, "n_tricks must be at least 1");
        let max_tricks = max_tricks_for(num_players);
        assert!(
            n_tricks <= max_tricks,
            "n_tricks {} exceeds the max for {} players ({})",
            n_tricks,
            num_players,
            max_tricks
        );
        assert!(
            num_players * n_tricks + 1 <= OH_DECK_SIZE,
            "deck too small for {} tricks * {} players",
            n_tricks,
            num_players
        );

        OhHellGameState {
            num_players: num_players as u8,
            n_tricks: n_tricks as u8,
            cur_player: 0,
            phase: OHPhase::DealHands,
            hands: [0; MAX_PLAYERS],
            face_up: None,
            trump_suit: None,
            bids: [None; MAX_PLAYERS],
            num_bids: 0,
            trick_cards: [None; MAX_PLAYERS],
            num_in_trick: 0,
            trick_starter: 0,
            trick_winners: Vec::new(),
            tricks_won: [0; MAX_PLAYERS],
            cards_played: 0,
            played_mask: 0,
            key: IStateKey::default(),
            play_order: Vec::new(),
        }
    }

    /// Build a `Game<OhHellGameState>` for the given `(num_players, n_tricks)`.
    /// `Game::new` requires a function pointer, so we dispatch to one of a
    /// pre-enumerated set of closures. Supported: 2 ≤ num_players ≤ 7,
    /// 1 ≤ n_tricks ≤ `max_tricks_for(num_players)`.
    pub fn game(num_players: usize, n_tricks: usize) -> Game<OhHellGameState> {
        let new_f: fn() -> OhHellGameState = match (num_players, n_tricks) {
            (2, 1) => || OhHell::new_state(2, 1),
            (2, 2) => || OhHell::new_state(2, 2),
            (2, 3) => || OhHell::new_state(2, 3),
            (2, 4) => || OhHell::new_state(2, 4),
            (2, 5) => || OhHell::new_state(2, 5),
            (2, 6) => || OhHell::new_state(2, 6),
            (2, 7) => || OhHell::new_state(2, 7),
            (2, 8) => || OhHell::new_state(2, 8),
            (2, 9) => || OhHell::new_state(2, 9),
            (2, 10) => || OhHell::new_state(2, 10),
            (3, 1) => || OhHell::new_state(3, 1),
            (3, 2) => || OhHell::new_state(3, 2),
            (3, 3) => || OhHell::new_state(3, 3),
            (3, 4) => || OhHell::new_state(3, 4),
            (3, 5) => || OhHell::new_state(3, 5),
            (3, 6) => || OhHell::new_state(3, 6),
            (3, 7) => || OhHell::new_state(3, 7),
            (3, 8) => || OhHell::new_state(3, 8),
            (3, 9) => || OhHell::new_state(3, 9),
            (3, 10) => || OhHell::new_state(3, 10),
            (4, 1) => || OhHell::new_state(4, 1),
            (4, 2) => || OhHell::new_state(4, 2),
            (4, 3) => || OhHell::new_state(4, 3),
            (4, 4) => || OhHell::new_state(4, 4),
            (4, 5) => || OhHell::new_state(4, 5),
            (4, 6) => || OhHell::new_state(4, 6),
            (4, 7) => || OhHell::new_state(4, 7),
            _ => panic!(
                "unsupported (num_players, n_tricks) combination: ({}, {})",
                num_players, n_tricks
            ),
        };
        Game {
            new: Box::new(new_f),
            max_players: num_players,
            max_actions: OH_DECK_SIZE + n_tricks + 1,
        }
    }

    /// Construct a state from a sequence of actions. Useful for tests.
    pub fn from_actions(
        num_players: usize,
        n_tricks: usize,
        actions: &[OHAction],
    ) -> OhHellGameState {
        let mut gs = OhHell::new_state(num_players, n_tricks);
        for &a in actions {
            gs.apply_action(a.into());
        }
        gs
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OhHellGameState {
    /// Number of players in this hand (2..=MAX_PLAYERS).
    num_players: u8,
    n_tricks: u8,
    cur_player: Player,
    phase: OHPhase,

    /// Bitmask of cards held in each player's hand, indexed by `OHCard as u8`.
    /// 52-bit mask packed into u64 (bits 0..52 used).
    /// Only the first `num_players` slots are live.
    hands: [u64; MAX_PLAYERS],
    face_up: Option<OHCard>,
    trump_suit: Option<OHSuit>,

    bids: [Option<u8>; MAX_PLAYERS],
    num_bids: u8,

    /// Cards in the trick currently in progress (slot per position in the
    /// trick — 0 = trick starter, 1 = next, ...).
    trick_cards: [Option<OHCard>; MAX_PLAYERS],
    num_in_trick: u8,
    trick_starter: Player,

    /// Winner of each completed trick, in order.
    trick_winners: Vec<Player>,
    tricks_won: [u8; MAX_PLAYERS],
    cards_played: u8,

    /// Bitmask of every card that has been played so far across all
    /// completed tricks AND the in-progress trick. Maintained
    /// incrementally on `apply_play` and `undo`, so consumers (e.g. the
    /// open-hand solver's action processor) can query "what's visible
    /// on the table" without scanning the action history.
    played_mask: u64,

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

    pub fn bids(&self) -> &[Option<u8>] {
        &self.bids[..self.num_players as usize]
    }

    pub fn tricks_won(&self) -> &[u8] {
        &self.tricks_won[..self.num_players as usize]
    }

    pub fn cards_played(&self) -> usize {
        self.cards_played as usize
    }

    pub fn get_hand(&self, player: Player) -> Vec<OHCard> {
        cards_from_mask(self.hands[player])
    }

    /// Bit mask of `player`'s current hand (zero-alloc). One bit per
    /// `OHCard as u8`. Intended for the open-hand solver's hot path.
    pub fn hand_mask(&self, player: Player) -> u64 {
        self.hands[player]
    }

    /// Bit mask of every card on the table (played in completed tricks or
    /// in the trick currently in progress). Updated incrementally so this
    /// is O(1) regardless of game length.
    pub fn played_mask(&self) -> u64 {
        self.played_mask
    }

    /// Cards in the in-progress trick, in play order (length `num_in_trick`).
    pub fn current_trick(&self) -> &[Option<OHCard>] {
        &self.trick_cards[..self.num_players as usize]
    }

    pub fn num_in_trick(&self) -> usize {
        self.num_in_trick as usize
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
        let np = self.num_players as usize;
        let p = self.cur_player;
        self.deal_card(p, card);

        let dealt_so_far = self.key.len() + 1;
        if dealt_so_far == np * self.n_tricks as usize {
            self.phase = OHPhase::DealFaceUp;
            self.cur_player = 0;
        } else {
            self.cur_player = (p + 1) % np;
        }
    }

    fn apply_deal_face_up(&mut self, card: OHCard) {
        self.face_up = Some(card);
        self.trump_suit = Some(card.suit());
        self.phase = OHPhase::Bidding;
        self.cur_player = 0;
    }

    fn apply_bid(&mut self, n: u8) {
        let np = self.num_players as usize;
        debug_assert!(n <= self.n_tricks, "bid out of range");
        let p = self.cur_player;
        debug_assert!(self.bids[p].is_none());
        self.bids[p] = Some(n);
        self.num_bids += 1;

        if self.num_bids as usize == np {
            self.phase = OHPhase::Play;
            self.cur_player = 0;
            self.trick_starter = 0;
            self.num_in_trick = 0;
        } else {
            self.cur_player = (p + 1) % np;
        }
    }

    fn apply_play(&mut self, card: OHCard) {
        let np = self.num_players as usize;
        let p = self.cur_player;
        self.remove_card(p, card);
        self.trick_cards[self.num_in_trick as usize] = Some(card);
        self.played_mask |= 1u64 << (card as u8);
        self.num_in_trick += 1;
        self.cards_played += 1;

        if self.num_in_trick as usize == np {
            let winner = self.trick_winner();
            self.trick_winners.push(winner);
            self.tricks_won[winner] += 1;
            self.trick_cards = [None; MAX_PLAYERS];
            self.num_in_trick = 0;
            self.trick_starter = winner;
            self.cur_player = winner;

            if self.cards_played as usize == np * self.n_tricks as usize {
                self.phase = OHPhase::Terminal;
            }
        } else {
            self.cur_player = (p + 1) % np;
        }
    }

    /// Determine the winner of the just-completed trick.
    fn trick_winner(&self) -> Player {
        let np = self.num_players as usize;
        let trump = self.trump_suit.expect("trump must be set in play phase");
        let lead = self.trick_cards[0].expect("trick has plays").suit();

        let mut best_pos = 0usize;
        let mut best_card = self.trick_cards[0].unwrap();
        for i in 1..np {
            let c = self.trick_cards[i].expect("trick fully played");
            if beats(c, best_card, lead, trump) {
                best_card = c;
                best_pos = i;
            }
        }
        (self.trick_starter + best_pos) % np
    }

    /// Build the score vector. Each player who made their bid exactly scores
    /// `10 + bid`; everyone else gets 0. Slots beyond `num_players` are 0.
    fn raw_scores(&self) -> [f64; MAX_PLAYERS] {
        let mut out = [0.0; MAX_PLAYERS];
        for p in 0..(self.num_players as usize) {
            let bid = self.bids[p].expect("bids set when evaluating");
            if self.tricks_won[p] == bid {
                out[p] = 10.0 + bid as f64;
            }
        }
        out
    }
}

/// Per-suit card masks indexed by `OHSuit as u8` (Spades, Clubs, Hearts,
/// Diamonds). 13 bits set per suit in their canonical positions within
/// the 52-card deck.
pub(super) const SUIT_MASK: [u64; 4] = {
    let mut out = [0u64; 4];
    let mut i = 0;
    while i < OH_DECK_SIZE {
        let suit_idx = (i / 13) as usize;
        out[suit_idx] |= 1u64 << i;
        i += 1;
    }
    out
};

fn mask_for_suit(suit: OHSuit) -> u64 {
    SUIT_MASK[suit as usize]
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
        let np = self.num_players as usize;
        let scores = self.raw_scores();
        let mean: f64 = scores[..np].iter().sum::<f64>() / np as f64;
        scores[p] - mean
    }

    fn istate_key(&self, player: Player) -> IStateKey {
        let np = self.num_players as usize;
        let mut k = IStateKey::default();
        let deal_count = np * self.n_tricks as usize;
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
        let np = self.num_players as usize;
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
        let bids_end = (bids_start + np).min(k.len());
        if bids_end > bids_start {
            s.push('|');
            for i in bids_start..bids_end {
                write!(s, "{}", OHAction::from(k[i])).unwrap();
            }
        }

        // Plays, broken into tricks of np cards.
        let plays_start = bids_start + np;
        if k.len() > plays_start {
            let mut i = plays_start;
            while i < k.len() {
                if (i - plays_start) % np == 0 {
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
        self.num_players as usize
    }

    fn cur_player(&self) -> Player {
        self.cur_player
    }

    fn key(&self) -> IStateKey {
        // Canonicalize deal order: sort each player's deal segment so isomorphic
        // states share a key.
        let np = self.num_players as usize;
        let mut sorted = self.key;
        let deal_count = np * self.n_tricks as usize;
        if sorted.len() >= np {
            for p in 0..np {
                let mut cards: Vec<Action> = (0..self.n_tricks as usize)
                    .map(|t| t * np + p)
                    .filter(|i| *i < deal_count.min(sorted.len()))
                    .map(|i| sorted[i])
                    .collect();
                cards.sort();
                for (t, a) in cards.iter().enumerate() {
                    let idx = t * np + p;
                    if idx < sorted.len() {
                        sorted[idx] = *a;
                    }
                }
            }
        }
        sorted
    }

    /// Isomorphic transposition table hash. Two states that are
    /// equivalent under the open-hand alpha-beta search produce the same
    /// hash, so the TT cache shares entries across symmetry classes.
    ///
    /// Symmetries exploited (only valid at start-of-trick, which is the
    /// only time we cache anyway):
    ///   * **Non-trump suit permutation**: the three non-trump suits are
    ///     functionally interchangeable when no lead suit is in play. The
    ///     trump suit's signature is fixed in slot 0; the other three
    ///     suits' signatures are sorted ascending.
    ///   * **Rank compaction within a suit**: only the relative rank
    ///     among in-play cards matters for trick-taking outcomes. We walk
    ///     in-play cards in ascending absolute-rank order and record the
    ///     owner at each position; absolute rank labels drop out.
    ///
    /// Player identity and trump-suit identity are *not* canonicalised:
    /// bids, tricks-won and seat order make players distinguishable, and
    /// trump suit changes the value of every other card so it can't be
    /// swapped.
    fn transposition_table_hash(&self) -> Option<crate::istate::IsomorphicHash> {
        if self.phase != OHPhase::Play {
            return None;
        }
        // Mid-trick caching isn't supported: the trick's partial state
        // would push us into many near-identical entries with no real
        // reuse, plus the lead-suit symmetry doesn't hold here.
        if self.num_in_trick != 0 {
            return None;
        }

        let np = self.num_players as usize;
        let trump = self.trump_suit.expect("trump set in Play");
        let face_up_mask = self.face_up.map(|c| 1u64 << (c as u8)).unwrap_or(0);
        let used_mask = self.played_mask | face_up_mask;

        // Per-suit signature: 4 bits per in-play card encoding the owner
        // (0..np for players, np for stock), in ascending absolute-rank
        // order. With 13 cards per suit this is at most 52 bits, fits in
        // a u64.
        let mut suit_sigs = [0u64; 4];
        for (suit_idx, sig) in suit_sigs.iter_mut().enumerate() {
            let suit_full = SUIT_MASK[suit_idx];
            let in_play = suit_full & !used_mask;

            let mut s = 0u64;
            let mut pos: u32 = 0;
            let mut m = in_play;
            while m != 0 {
                let card_bit = m & m.wrapping_neg();
                m &= m - 1;
                let mut owner = np as u64;
                for p in 0..np {
                    if self.hands[p] & card_bit != 0 {
                        owner = p as u64;
                        break;
                    }
                }
                s |= owner << (pos * 4);
                pos += 1;
            }
            *sig = s;
        }

        // Trump's signature goes in slot 0; the three non-trump
        // signatures sort ascending so any permutation collapses.
        let trump_sig = suit_sigs[trump as usize];
        let mut other = [0u64; 3];
        let mut idx = 0;
        for s in 0..4 {
            if s != trump as usize {
                other[idx] = suit_sigs[s];
                idx += 1;
            }
        }
        other.sort();

        const K: u64 = 0x517cc1b727220a95;
        #[inline(always)]
        fn mix(h: u64, x: u64) -> u64 {
            (h.rotate_left(5) ^ x).wrapping_mul(K)
        }

        let mut h: u64 = 0;
        h = mix(h, trump_sig);
        h = mix(h, other[0]);
        h = mix(h, other[1]);
        h = mix(h, other[2]);

        // Per-player counters and the seat we're cycling on.
        let mut bid_pack = 0u64;
        let mut tricks_pack = 0u64;
        for p in 0..np {
            bid_pack |= (self.bids[p].unwrap_or(255) as u64) << (p * 8);
            tricks_pack |= (self.tricks_won[p] as u64) << (p * 8);
        }
        h = mix(h, bid_pack);
        h = mix(h, tricks_pack);
        let tail = (self.cur_player as u64 & 0xff) | ((self.num_players as u64) << 8);
        h = mix(h, tail);
        Some(h)
    }

    fn undo(&mut self) {
        let np = self.num_players as usize;
        let last_player = self.play_order.pop().expect("non-empty play_order");
        let last_action = self.key.pop();
        let oa = OHAction::from(last_action);

        let n_after = self.key.len();
        let deal_count = np * self.n_tricks as usize;

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
        } else if n_after < deal_count + 1 + np {
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

                let plays_remaining = np - 1;
                self.num_in_trick = plays_remaining as u8;
                self.trick_cards = [None; MAX_PLAYERS];
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
                self.played_mask &= !(1u64 << (card as u8));
                self.cur_player = last_player;
                self.cards_played -= 1;
            } else {
                self.num_in_trick -= 1;
                self.trick_cards[self.num_in_trick as usize] = None;
                self.deal_card(last_player, card);
                self.played_mask &= !(1u64 << (card as u8));
                self.cur_player = last_player;
                self.cards_played -= 1;
            }
        }
    }
}

impl Display for OhHellGameState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // [Hands]|FaceUp|Bids|Trick1|Trick2|...
        let np = self.num_players as usize;
        let deal_count = np * self.n_tricks as usize;
        let n_hand = self.n_tricks as usize;

        for p in 0..np {
            if p > 0 {
                f.write_char('|')?;
            }
            let mut cards: Vec<OHCard> = (0..n_hand)
                .map(|t| t * np + p)
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
        let bids_end = (bids_start + np).min(self.key.len());
        if bids_end > bids_start {
            f.write_char('|')?;
            for i in bids_start..bids_end {
                write!(f, "{}", OHAction::from(self.key[i]))?;
            }
        }

        let plays_start = bids_start + np;
        let mut i = plays_start;
        while i < self.key.len() {
            if (i - plays_start) % np == 0 {
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
        let np = self.num_players as usize;
        let n_tricks = self.n_tricks as usize;
        let deal_count = np * n_tricks;

        // ---- 1. Gather public info ----

        let player_initial_cards: Vec<OHCard> = (0..n_tricks)
            .map(|t| t * np + player)
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
        let mut played_mask = [0u64; MAX_PLAYERS];
        let plays_start = deal_count + 1 + np;
        let mut trick_lead_suit: Option<OHSuit> = None;
        let mut trick_pos: usize = 0;
        let mut forbidden_suits: [u8; MAX_PLAYERS] = [0; MAX_PLAYERS];

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
            if trick_pos == np {
                trick_pos = 0;
                trick_lead_suit = None;
            }
        }

        // ---- 2. Build the unknown pool and per-player budgets ----

        let played_count_p = |p: Player| -> usize { played_mask[p].count_ones() as usize };

        let mut budgets: [usize; MAX_PLAYERS] = [0; MAX_PLAYERS];
        for q in 0..np {
            if q != player {
                budgets[q] = n_tricks.saturating_sub(played_count_p(q));
            }
        }

        let revealed: u64 = player_initial_mask
            | face_up_mask
            | played_mask[..np].iter().fold(0u64, |a, b| a | b);
        let unknown_pool: Vec<OHCard> = OH_DECK
            .iter()
            .copied()
            .filter(|c| revealed & (1u64 << (*c as u8)) == 0)
            .collect();

        let total_budget: usize = budgets[..np].iter().sum();
        debug_assert!(
            total_budget <= unknown_pool.len(),
            "constraint inference failure: need {} cards, pool has {}",
            total_budget,
            unknown_pool.len()
        );

        // ---- 3. Constraint-propagating backtracking solver ----

        debug_assert!(
            per_suit_feasibility(
                np,
                &unknown_pool,
                &budgets,
                &forbidden_suits,
                total_budget,
                player,
            ),
            "constraint inference produced an infeasible per-suit assignment"
        );

        let pool_order = build_constrained_pool_order(
            np,
            &unknown_pool,
            &forbidden_suits,
            player,
            rng,
        );

        let mut assignment: [Vec<OHCard>; MAX_PLAYERS] = Default::default();

        let success = solve_assignment(
            np,
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
        let mut deal_pool: [Vec<OHCard>; MAX_PLAYERS] = Default::default();
        for q in 0..np {
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

        let mut ngs = OhHell::new_state(np, n_tricks);

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
    num_players: usize,
    pool: &[OHCard],
    budgets: &[usize; MAX_PLAYERS],
    forbidden: &[u8; MAX_PLAYERS],
    needed: usize,
    skip_player: Player,
) -> bool {
    let stock_slots = pool.len().saturating_sub(needed);
    for suit in OHSuit::ALL {
        let bit = suit_bit(suit);
        let cards_in_suit = pool.iter().filter(|c| c.suit() == suit).count();
        let mut eligible_slots: usize = 0;
        for p in 0..num_players {
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
    num_players: usize,
    pool: &[OHCard],
    forbidden: &[u8; MAX_PLAYERS],
    skip_player: Player,
    rng: &mut T,
) -> Vec<OHCard> {
    use rand::seq::SliceRandom;

    let mut eligible_count = [0u8; 4];
    for (i, suit) in OHSuit::ALL.iter().enumerate() {
        let bit = suit_bit(*suit);
        for p in 0..num_players {
            if p == skip_player {
                continue;
            }
            if forbidden[p] & bit == 0 {
                eligible_count[i] += 1;
            }
        }
    }

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
    num_players: usize,
    assignment: &mut [Vec<OHCard>; MAX_PLAYERS],
    budgets: &[usize; MAX_PLAYERS],
    forbidden: &[u8; MAX_PLAYERS],
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
        for p in 0..num_players {
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
    for p in 0..num_players {
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
            num_players,
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
            num_players,
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
        let gs = OhHell::new_state(3, 2);
        assert_eq!(gs.phase(), OHPhase::DealHands);
        assert!(gs.is_chance_node());
        assert!(!gs.is_terminal());
        assert_eq!(gs.num_players(), 3);
    }

    #[test]
    fn dealing_legal_actions_are_remaining_cards() {
        let gs = OhHell::new_state(3, 2);
        let a = actions!(gs);
        assert_eq!(a.len(), OH_DECK_SIZE, "fresh deck should have 52 cards");
    }

    #[test]
    fn dealing_does_not_repeat_cards() {
        let mut gs = OhHell::new_state(3, 2);
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
        let mut gs = OhHell::new_state(3, n_tricks);
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
        let mut gs = OhHell::new_state(3, 2);
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
        for p in 0..3 {
            assert_eq!(gs.evaluate(p), 0.0);
        }
    }

    #[test]
    fn making_your_bid_pays_off() {
        let mut gs = OhHell::new_state(3, 1);
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
        let mut gs = OhHell::new_state(3, 1);
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
        let mut gs = OhHell::new_state(3, 1);
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
            let mut gs = OhHell::new_state(3, 2);
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
            let mut gs = OhHell::new_state(3, 2);
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
            let mut gs = OhHell::new_state(3, 2);
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
            let mut gs = OhHell::new_state(3, 2);
            while !gs.is_terminal() {
                let a = actions!(gs);
                let action = *a.choose(&mut rng).unwrap();
                gs.apply_action(action);
            }
            let total: f64 = (0..3).map(|p| gs.evaluate(p)).sum();
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
            let mut gs = OhHell::new_state(3, 2);
            while gs.is_chance_node() {
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
            // Through bidding.
            while gs.phase() == OHPhase::Bidding {
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
            for p in 0..3 {
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
            let mut gs = OhHell::new_state(3, 2);
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
        let mut gs = OhHell::new_state(3, 2);
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
        let mut gs = OhHell::new_state(3, 5);
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
        for p in 0..3 {
            let orig = gs.istate_key(p);
            for _ in 0..5 {
                let r = gs.resample_from_istate(p, &mut rng);
                assert_eq!(r.istate_key(p), orig);
            }
        }
    }

    // ---------------- 2-player Oh Hell ----------------

    #[test]
    fn two_player_basic_phases() {
        let gs = OhHell::new_state(2, 5);
        assert_eq!(gs.num_players(), 2);
        assert_eq!(gs.n_tricks(), 5);
        assert_eq!(gs.phase(), OHPhase::DealHands);
    }

    #[test]
    fn two_player_full_game_terminal_and_scoring() {
        // 2 players, 1 trick.
        //   P0: 9s,  P1: 9c, face up 9h (hearts trump).
        //   Bids: P0=0, P1=1. P0 leads 9s. P1 has no spades and no hearts;
        //   plays 9c. 9s is lead-suit highest → P0 wins.
        //   Final: P0 won 1 (bid 0) → bust → 0. P1 won 0 (bid 1) → 0.
        let mut gs = OhHell::new_state(2, 1);
        gs.apply_action(OHAction::Card(OHCard::NS).into()); // P0 deal
        gs.apply_action(OHAction::Card(OHCard::NC).into()); // P1 deal
        gs.apply_action(OHAction::Card(OHCard::NH).into()); // face up
        gs.apply_action(OHAction::Bid(0).into()); // P0
        gs.apply_action(OHAction::Bid(1).into()); // P1
        gs.apply_action(OHAction::Card(OHCard::NS).into()); // P0 leads
        gs.apply_action(OHAction::Card(OHCard::NC).into()); // P1 follows
        assert!(gs.is_terminal());
        assert_eq!(gs.tricks_won(), &[1, 0][..]);
        // Both score 0 (mean = 0), so evaluate is 0 for both.
        assert_eq!(gs.evaluate(0), 0.0);
        assert_eq!(gs.evaluate(1), 0.0);
    }

    #[test]
    fn two_player_both_make_bid() {
        // P0 bids 1, wins 1 → 11. P1 bids 0, wins 0 → 10.
        // Mean = 10.5. evaluate(0) = 0.5, evaluate(1) = -0.5. Zero-sum.
        let mut gs = OhHell::new_state(2, 1);
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::NC).into());
        gs.apply_action(OHAction::Card(OHCard::NH).into());
        gs.apply_action(OHAction::Bid(1).into());
        gs.apply_action(OHAction::Bid(0).into());
        gs.apply_action(OHAction::Card(OHCard::NS).into());
        gs.apply_action(OHAction::Card(OHCard::NC).into());
        assert!(gs.is_terminal());
        assert_eq!(gs.tricks_won(), &[1, 0][..]);
        assert!((gs.evaluate(0) - 0.5).abs() < 1e-9);
        assert!((gs.evaluate(1) + 0.5).abs() < 1e-9);
        assert!((gs.evaluate(0) + gs.evaluate(1)).abs() < 1e-9);
    }

    #[test]
    fn two_player_undo_round_trip_random() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xBEEF);
        for _ in 0..50 {
            let mut gs = OhHell::new_state(2, 3);
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
                    "2-player undo broken at state {}",
                    before
                );
            }
        }
    }

    #[test]
    fn two_player_resample_preserves_istate() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xF00D);
        for _ in 0..20 {
            let mut gs = OhHell::new_state(2, 3);
            while gs.is_chance_node() {
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
            while !gs.is_terminal() {
                let p = gs.cur_player();
                let orig = gs.istate_key(p);
                for _ in 0..3 {
                    let r = gs.resample_from_istate(p, &mut rng);
                    assert_eq!(r.istate_key(p), orig);
                }
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
        }
    }

    #[test]
    fn two_player_zero_sum_evaluation() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0xD06);
        for _ in 0..30 {
            let mut gs = OhHell::new_state(2, 3);
            while !gs.is_terminal() {
                let a = actions!(gs);
                gs.apply_action(*a.choose(&mut rng).unwrap());
            }
            let total: f64 = (0..2).map(|p| gs.evaluate(p)).sum();
            assert!(total.abs() < 1e-9, "2-player scores not zero-sum: {}", total);
        }
    }

    #[test]
    fn two_player_max_tricks_supported() {
        // With 2 players, max_tricks_for is 15 cards (deck of 52, 1 face up).
        assert_eq!(max_tricks_for(2), 15);
        assert_eq!(max_tricks_for(3), 10);
        assert_eq!(max_tricks_for(4), 7);
    }

    // ---------------- Isomorphic TT hash ----------------

    /// Helper: drive a 2-player 2-trick game through random chance +
    /// bidding into the play phase at a start-of-trick boundary, then
    /// return the resulting state so iso-hash tests can manipulate it.
    fn play_start_state(seed: u64) -> OhHellGameState {
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut gs = OhHell::new_state(2, 2);
        while gs.is_chance_node() {
            let a = actions!(gs);
            gs.apply_action(*a.choose(&mut rng).unwrap());
        }
        while gs.phase() == OHPhase::Bidding {
            let a = actions!(gs);
            gs.apply_action(*a.choose(&mut rng).unwrap());
        }
        gs
    }

    /// Swap all bits of two suits in a 52-bit mask (each suit owns 13
    /// consecutive bits at offset `suit_idx * 13`).
    fn swap_suit_bits_in_mask(m: u64, a: u8, b: u8) -> u64 {
        let base_a = (a as u64) * 13;
        let base_b = (b as u64) * 13;
        let mask_a = (((1u64 << 13) - 1) << base_a);
        let mask_b = (((1u64 << 13) - 1) << base_b);
        let bits_a = (m & mask_a) >> base_a;
        let bits_b = (m & mask_b) >> base_b;
        let cleared = m & !(mask_a | mask_b);
        cleared | (bits_a << base_b) | (bits_b << base_a)
    }

    /// Apply a non-trump suit swap to a play-phase state. Both hands and
    /// the played_mask have the bits of the two suits exchanged.
    fn permute_two_nontrump_suits(gs: &OhHellGameState, a: OHSuit, b: OHSuit) -> OhHellGameState {
        assert_ne!(a, b);
        assert_ne!(Some(a), gs.trump_suit);
        assert_ne!(Some(b), gs.trump_suit);
        let mut out = gs.clone();
        let ai = a as u8;
        let bi = b as u8;
        for p in 0..(gs.num_players as usize) {
            out.hands[p] = swap_suit_bits_in_mask(gs.hands[p], ai, bi);
        }
        out.played_mask = swap_suit_bits_in_mask(gs.played_mask, ai, bi);
        out
    }

    /// Iso hash must be the same when two non-trump suits are swapped —
    /// they're interchangeable at start-of-trick.
    #[test]
    fn iso_hash_collapses_nontrump_suit_swap() {
        for seed in 0..30u64 {
            let gs = play_start_state(seed);
            let trump = gs.trump_suit.unwrap();
            // Pick two non-trump suits.
            let nontrumps: Vec<OHSuit> =
                OHSuit::ALL.iter().copied().filter(|s| *s != trump).collect();
            let permuted = permute_two_nontrump_suits(&gs, nontrumps[0], nontrumps[1]);
            assert_eq!(
                gs.transposition_table_hash(),
                permuted.transposition_table_hash(),
                "iso hash should not depend on non-trump suit labels (seed={})",
                seed
            );
        }
    }

    /// Changing the trump suit (with the same card distribution) must
    /// produce a *different* iso hash — trump isn't interchangeable.
    #[test]
    fn iso_hash_separates_trump_change() {
        let gs = play_start_state(42);
        let mut alt = gs.clone();
        // Pick a non-trump suit and make it trump instead.
        let alt_trump = OHSuit::ALL
            .iter()
            .copied()
            .find(|s| Some(*s) != gs.trump_suit)
            .unwrap();
        alt.trump_suit = Some(alt_trump);
        assert_ne!(
            gs.transposition_table_hash(),
            alt.transposition_table_hash(),
            "iso hash must distinguish different trump suits"
        );
    }

    /// Moving a card from one player to another must change the iso
    /// hash — player identity isn't interchangeable.
    #[test]
    fn iso_hash_separates_owner_change() {
        let gs = play_start_state(11);
        // Find any card P0 holds and move it to P1's hand on a clone.
        let p0_hand = gs.hands[0];
        assert!(p0_hand != 0);
        let card_bit = p0_hand & p0_hand.wrapping_neg();
        let mut alt = gs.clone();
        alt.hands[0] &= !card_bit;
        alt.hands[1] |= card_bit;
        assert_ne!(
            gs.transposition_table_hash(),
            alt.transposition_table_hash(),
            "iso hash must distinguish a card moving from P0 to P1"
        );
    }

    /// Changing a bid must change the iso hash — bidding state is part
    /// of scoring and isn't part of the iso symmetry.
    #[test]
    fn iso_hash_separates_bid_change() {
        let gs = play_start_state(101);
        let original_bid = gs.bids[0].expect("bid set in play");
        let mut alt = gs.clone();
        alt.bids[0] = Some(if original_bid == 0 { 1 } else { 0 });
        assert_ne!(
            gs.transposition_table_hash(),
            alt.transposition_table_hash(),
            "iso hash must distinguish different bids"
        );
    }

    /// Iso hash returns `None` during bidding (consistent with the old
    /// behaviour — TT only caches play-phase states).
    #[test]
    fn iso_hash_none_during_bidding() {
        let mut rng: StdRng = SeedableRng::seed_from_u64(0);
        let mut gs = OhHell::new_state(2, 2);
        while gs.is_chance_node() {
            let a = actions!(gs);
            gs.apply_action(*a.choose(&mut rng).unwrap());
        }
        assert_eq!(gs.phase(), OHPhase::Bidding);
        assert_eq!(gs.transposition_table_hash(), None);
    }

    #[test]
    fn two_player_legal_bids_include_zero_to_n_tricks() {
        // 2 players, 4 tricks; after dealing + face up, P0 to bid → 5 options (0..=4).
        let mut gs = OhHell::new_state(2, 4);
        let mut acts = Vec::new();
        // Deal 8 cards.
        for c in OH_DECK.iter().take(8) {
            gs.apply_action(OHAction::Card(*c).into());
        }
        // Face up.
        gs.apply_action(OHAction::Card(OH_DECK[8]).into());
        assert_eq!(gs.phase(), OHPhase::Bidding);
        gs.legal_actions(&mut acts);
        assert_eq!(acts.len(), 5);
    }
}
