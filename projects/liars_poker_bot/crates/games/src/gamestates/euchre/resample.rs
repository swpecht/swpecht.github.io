use itertools::Itertools;
use rand::seq::SliceRandom;

use crate::{
    gamestates::euchre::{actions::EAction, suit_mask, EPhase, Euchre},
    pool::Pool,
    resample::ResampleFromInfoState,
    Action, GameState, Player,
};

use super::{actions::Card, deck::Hand, EuchreGameState};

/// Resample from info state method for euchre
///
/// This method discards a RANDOM card for the dealer if a discard is required.
/// It's not yet clear what impact this has on the results of downstream algorithms
impl ResampleFromInfoState for EuchreGameState {
    fn resample_from_istate<T: rand::Rng>(&self, player: Player, rng: &mut T) -> Self {
        if self.phase() == EPhase::DealHands || self.phase() == EPhase::DealFaceUp {
            panic!("don't yet support resampling of deal phase gamestates")
        }

        // Masks that track which cards players are allowed to have
        let mut allowed_cards = [Hand::all_cards(); 4];
        let mut known_cards = [Hand::default(); 4];
        let face_up = self.face_up().unwrap();
        let key = self.key();

        // collect the played cards from all players
        self.key
            .iter()
            .zip(self.play_order.iter())
            .skip(key.len() - self.cards_played)
            .map(|(a, p)| (EAction::from(*a).card(), p))
            .for_each(|(c, p)| known_cards[*p].add(c));

        // remove the face up card from every player
        allowed_cards.iter_mut().for_each(|p| p.remove(face_up));

        // and ensure the dealer isn't dealt the face up card, even if they played it
        known_cards[3].remove(face_up);

        // collect the players own dealt cards
        self.key
            .iter()
            .zip(self.play_order.iter())
            .take(20)
            .map(|(a, p)| (EAction::from(*a).card(), p))
            .filter(|(_, p)| **p == player)
            .for_each(|(c, _)| known_cards[player].add(c));

        // Remove a suit from allowed cards if player didn't previously follow suit
        let offset = key.len() - self.cards_played;
        for t in 0..5 {
            let trick_start = offset + t * 4;
            let lead = key.get(trick_start).map(|x| EAction::from(*x).card());
            if lead.is_none() {
                break;
            }

            let lead_player = self.play_order[trick_start];
            let lead_suit = self.get_suit(lead.unwrap());

            for i in 1..3 {
                if let Some(played_card) =
                    key.get(trick_start + i).map(|x| EAction::from(*x).card())
                {
                    let played_suit = self.get_suit(played_card);
                    if played_suit != lead_suit {
                        let suit_cards = suit_mask(lead_suit, self.trump);
                        allowed_cards[(lead_player + i) % 4].remove_all(suit_cards);
                    }
                }
            }
        }

        // remove the known cards for all players
        let mut all_known = Hand::default();
        known_cards.iter().for_each(|x| all_known.add_all(*x));
        allowed_cards
            .iter_mut()
            .for_each(|x| x.remove_all(all_known));

        // ensure constraints give us enough cards to deal
        // otherwise it's unsolvable
        assert!(
            known_cards
                .iter()
                .zip(allowed_cards.iter())
                // only check the first 3 players, player 4 can be unsolvable given then can discard a card
                .take(3)
                .all(|(k, a)| k.len() + a.len() >= 5),
            "Constraints aren't solvable. \nknown cards: {:?}\nallowed cards: {:?}",
            known_cards,
            allowed_cards
        );

        let mut ngs = Euchre::new_state();
        let mut pool = Pool::new(Vec::new);
        assert!(
            search_for_deal(&mut ngs, known_cards, allowed_cards, face_up, 0, rng, &mut pool),
            "Failed to find a valid deal for resample of {} for {}\nknown cards: {:?}\nallowed cards: {:?}",
            self,
            player, known_cards, allowed_cards
        );
        let mut actions = Vec::new();

        // deal the face up
        ngs.apply_action(EAction::from(face_up).into());

        // apply the non-deal actions
        let mut is_last_pickup = false;
        for a in &self.key()[21..] {
            // handle the discard case. We randomly select a card to discard that isn't seen later. TBD
            // what impact this random discarding has on the sampling because an actual player would not discard a
            // card randomly.
            //
            // If it's not a discard, we apply the actions in the order we saw them.
            // discard is the only private action after deal phase
            if is_last_pickup && player != 3 {
                assert_eq!(ngs.cur_player(), 3);

                let played_cards = self
                    .key
                    .iter()
                    .skip(key.len() - self.cards_played)
                    .map(|a| EAction::from(*a).card())
                    .collect_vec();

                ngs.legal_actions(&mut actions);
                actions.shuffle(rng);
                for da in actions.iter().map(|x| EAction::from(*x)) {
                    let card = da.card();
                    if !played_cards.contains(&card) {
                        ngs.apply_action(da.into());
                        break;
                    }
                }
            } else {
                ngs.apply_action(*a);
            }

            is_last_pickup = false;
            if EAction::from(*a) == EAction::Pickup {
                is_last_pickup = true;
            }
        }

        ngs
    }
}

/// Searches the game tree for a deal that meets all constraints
fn search_for_deal<T: rand::Rng>(
    gs: &mut EuchreGameState,
    known: [Hand; 4],
    allowed: [Hand; 4],
    face_up: Card,
    depth: usize,
    rng: &mut T,
    pool: &mut Pool<Vec<Action>>,
) -> bool {
    if !meets_constraints(gs, known, allowed) {
        return false;
    }

    if depth == 20 {
        return true;
    }

    let mut actions = pool.detach();
    gs.legal_actions(&mut actions);
    actions.shuffle(rng);

    // move a known card to the front if one exists
    let cur_player = gs.cur_player();
    let idx = actions
        .iter()
        .map(|x| EAction::from(*x).card())
        .position(|x| known[cur_player].contains(x))
        .unwrap_or(0);
    actions.swap(0, idx);

    // filter out illegal moves
    actions.retain(|x| {
        let c = EAction::from(*x).card();
        known[cur_player].contains(c) || allowed[cur_player].contains(c)
    });

    // We're in a situation where the dealer needs to be dealt a card
    // they will ultimately discard
    //
    // We can give them any remaining card and skip constraint checking
    if actions.is_empty() && depth == 19 {
        assert_eq!(gs.cur_player, 3);
        gs.legal_actions(&mut actions);
        // don't deal the faceup card
        actions.retain(|x| EAction::from(*x).card() != face_up);
        let a = actions.choose(rng).unwrap();
        gs.apply_action(*a);
        pool.attach(actions);
        return true;
    }

    for a in actions.iter() {
        gs.apply_action(*a);
        if !search_for_deal(gs, known, allowed, face_up, depth + 1, rng, pool) {
            gs.undo()
        } else {
            pool.attach(actions);
            return true;
        }
    }

    pool.attach(actions);
    false
}

fn meets_constraints(gs: &EuchreGameState, known: [Hand; 4], allowed: [Hand; 4]) -> bool {
    for p in 0..4 {
        let hand = gs.deck.get_all(p.into());
        let all_allowed = hand
            .into_iter()
            .all(|c| allowed[p].contains(c) || known[p].contains(c));
        if !all_allowed {
            return false;
        }

        let dealt_known = hand.into_iter().filter(|c| known[p].contains(*c)).count();
        let dealt_known_first = dealt_known == hand.len() || dealt_known == known[p].len();
        if !dealt_known_first {
            return false;
        }
    }

    true
}
