use crate::game::GameState;

/// Implementation of chance sampled CFR
///
/// Based on implementation from: http://mlanctot.info/
/// cfrcs.cpp

pub fn cfrcs<T: GameState>(g: &T, depth: usize, reach0: f64, reach1: f64) -> Vec<f32> {
    if g.is_terminal() {
        return g.evaluate();
    }

    // No support for chance nodes yet
    assert!(!g.is_chance_node());

    //     // declare the variables
    //   Infoset is;
    //   unsigned long long infosetkey = 0;
    //   double stratEV = 0.0;
    //   int action = -1;

    //   int maxBid = (gs.curbid == 0 ? BLUFFBID-1 : BLUFFBID);
    //   int actionshere = maxBid - gs.curbid;
    //   assert(actionshere > 0);
    //   double moveEVs[actionshere];
    //   for (int i = 0; i < actionshere; i++)
    //     moveEVs[i] = 0.0;

    //   // get the info set (also set is.curMoveProbs using regret matching)
    //   getInfoset(gs, player, bidseq, is, infosetkey, actionshere);

    //   // iterate over the actions
    //   for (int i = gs.curbid+1; i <= maxBid; i++)
    //   {
    //     // there is a valid action here
    //     action++;
    //     assert(action < actionshere);

    //     unsigned long long newbidseq = bidseq;
    //     double moveProb = is.curMoveProbs[action];
    //     double newreach1 = (player == 1 ? moveProb*reach1 : reach1);
    //     double newreach2 = (player == 2 ? moveProb*reach2 : reach2);

    //     GameState ngs = gs;
    //     ngs.prevbid = gs.curbid;
    //     ngs.curbid = i;
    //     ngs.callingPlayer = player;
    //     newbidseq |= (1ULL << (BLUFFBID-i));

    //     double payoff = cfrcs(ngs, 3-player, depth+1, newbidseq, newreach1, newreach2, phase, updatePlayer);

    //     moveEVs[action] = payoff;
    //     stratEV += moveProb*payoff;
    //   }

    //   // post-traversals: update the infoset
    //   double myreach = (player == 1 ? reach1 : reach2);
    //   double oppreach = (player == 1 ? reach2 : reach1);

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

    todo!();
}
