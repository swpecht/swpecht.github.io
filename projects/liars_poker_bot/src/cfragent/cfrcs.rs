use crate::game::GameState;

/// Implementation of chance sampled CFR
///
/// Based on implementation from: http://mlanctot.info/
/// cfrcs.cpp

pub fn cfrcs<T: GameState>(gs: &T, _depth: usize, reach0: f64, reach1: f64) -> f64 {
    let cur_player = gs.cur_player();

    // if (terminal(gs))
    // {
    //   return payoff(gs, updatePlayer);
    // }
    if gs.is_terminal() {
        return gs.evaluate()[cur_player].into();
    }

    // No support for chance nodes yet
    assert!(!gs.is_chance_node());

    // declare the variables

    //   Infoset is;

    // unsigned long long infosetkey = 0;
    let _key = gs.istate_key(cur_player);

    //   double stratEV = 0.0;
    let strat_ev = 0.0;
    //   int action = -1;

    //   int maxBid = (gs.curbid == 0 ? BLUFFBID-1 : BLUFFBID);
    //   int actionshere = maxBid - gs.curbid;
    let actions = gs.legal_actions();

    //   assert(actionshere > 0);
    //   double moveEVs[actionshere];
    let mut move_evs: Vec<f64> = Vec::new();

    //   for (int i = 0; i < actionshere; i++)
    for _ in 0..actions.len() {
        move_evs.push(0.0);
    }

    // get the info set (also set is.curMoveProbs using regret matching)
    //   getInfoset(gs, player, bidseq, is, infosetkey, actionshere);

    //   // iterate over the actions
    //   for (int i = gs.curbid+1; i <= maxBid; i++)
    //   {
    for i in 0..actions.len() {
        let a = actions[i];

        //     unsigned long long newbidseq = bidseq;
        //     double moveProb = is.curMoveProbs[action];
        let moveProb: f64 = todo!();

        //     double newreach1 = (player == 1 ? moveProb*reach1 : reach1);
        //     double newreach2 = (player == 2 ? moveProb*reach2 : reach2);
        let mut newreach0 = reach0;
        let mut newreach1 = reach1;

        match cur_player {
            0 | 2 => newreach0 = reach0 * moveProb,
            1 | 3 => newreach1 = reach1 * moveProb,
            _ => panic!("invalid player"),
        };

        //     GameState ngs = gs;
        //     ngs.prevbid = gs.curbid;
        //     ngs.curbid = i;
        //     ngs.callingPlayer = player;
        //     newbidseq |= (1ULL << (BLUFFBID-i));
        let mut ngs = gs.clone();
        ngs.apply_action(a);

        //     double payoff = cfrcs(ngs, 3-player, depth+1, newbidseq, newreach1, newreach2, phase, updatePlayer);
        let payoff = cfrcs(&ngs, _depth + 1, newreach0, newreach1);

        //     moveEVs[action] = payoff;
        move_evs[i] = payoff;

        //     stratEV += moveProb*payoff;
        strat_ev += moveProb * payoff;

        //   }
    }

    //   // post-traversals: update the infoset
    //   double myreach = (player == 1 ? reach1 : reach2);
    let myreach = match cur_player {
        0 | 2 => reach0,
        1 | 3 => reach1,
        _ => panic!("invalid player: {}", cur_player),
    };
    //   double oppreach = (player == 1 ? reach2 : reach1);
    let oppreach = match cur_player {
        0 | 2 => reach1,
        1 | 3 => reach0,
        _ => panic!("invalid player: {}", cur_player),
    };

    //   if (phase == 1 && player == updatePlayer) // regrets
    //   {
    //     for (int a = 0; a < actionshere; a++)
    //     {
    //       // notice no chanceReach included here, unlike in Vanilla CFR
    //       // because it gets cancelled with q(z) in the denominator
    //       is.cfr[a] += oppreach*(moveEVs[a] - stratEV);
    //     }
    //   }

    //   if (phase >= 1 && player == updatePlayer) // av. strat
    //   {
    //     for (int a = 0; a < actionshere; a++)
    //     {
    //       is.totalMoveProbs[a] += myreach*is.curMoveProbs[a];
    //     }
    //   }

    //   // save the infoset back to the store if needed
    //   if (player == updatePlayer) {
    //     iss.put(infosetkey, is, actionshere, 0);
    //   }

    //   return stratEV;
    return strat_ev;
}
