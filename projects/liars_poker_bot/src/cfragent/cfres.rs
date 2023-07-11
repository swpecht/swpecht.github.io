use rand::{rngs::StdRng, seq::SliceRandom};

use crate::{
    alloc::Pool,
    database::NodeStore,
    game::{Action, GameState, Player},
};

use super::{cfr::Algorithm, cfrnode::CFRNode};

/// Implementation of external sampled CFR
///
/// Based on implementation from: http://mlanctot.info/
/// cfres.cpp
pub struct CFRES {
    nodes_touched: usize,
    rng: StdRng,
    vector_pool: Pool<Vec<Action>>,
}

impl Algorithm for CFRES {
    fn run<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
    ) {
        self.cfres(ns, &mut gs.clone(), update_player, 0);
    }

    fn nodes_touched(&self) -> usize {
        self.nodes_touched
    }
}

impl CFRES {
    fn cfres<G: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        _ns: &mut N,
        gs: &mut G,
        update_player: Player,
        _depth: usize,
    ) -> f64 {
        if gs.is_terminal() {
            return gs.evaluate(update_player);
        }

        self.nodes_touched += 1;

        let _cur_player = gs.cur_player();
        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);
        if actions.len() == 1 {
            // avoid processing nodes with no choices
            gs.apply_action(actions[0]);
            let v = self.cfres(_ns, gs, update_player, _depth + 1);
            gs.undo();
            return v;
        }

        if gs.is_chance_node() {
            let a = *actions.choose(&mut self.rng).unwrap();
            gs.apply_action(a);
            let v = self.cfres(_ns, gs, update_player, _depth + 1);
            gs.undo();
            return v;
        }

        //   // declare the variables
        //   Infoset is;
        //   unsigned long long infosetkey = 0;
        //   int action = -1;

        //   int maxBid = (gs.curbid == 0 ? BLUFFBID-1 : BLUFFBID);
        //   int actionshere = maxBid - gs.curbid;
        //   assert(actionshere > 0);

        //   double moveEVs[actionshere];
        //   for (int i = 0; i < actionshere; i++)
        //     moveEVs[i] = 0.0;

        //   // get the info set (also set is.curMoveProbs using regret matching)
        //   getInfoset(gs, player, bidseq, is, infosetkey, actionshere);

        //   double stratEV = 0.0;

        //   // traverse or sample actions.
        //   if (player != updatePlayer)
        //   {
        //     // sample opponent nodes
        //     double sampleprob = -1;
        //     int takeAction = sampleAction(is, actionshere, sampleprob, 0.0, false);
        //     CHKPROBNZ(sampleprob);

        //     // take the action. find the i for this action
        //     int i;
        //     for (i = gs.curbid+1; i <= maxBid; i++)
        //     {
        //       action++;

        //       if (action == takeAction)
        //         break;
        //     }

        //     assert(i >= gs.curbid+1 && i <= maxBid);

        //     unsigned long long newbidseq = bidseq;
        //     double moveProb = is.curMoveProbs[action];

        //     CHKPROBNZ(moveProb);

        //     //double newreach1 = (player == 1 ? moveProb*reach1 : reach1);
        //     //double newreach2 = (player == 2 ? moveProb*reach2 : reach2);

        //     GameState ngs = gs;
        //     ngs.prevbid = gs.curbid;
        //     ngs.curbid = i;
        //     ngs.callingPlayer = player;
        //     newbidseq |= (1 << (BLUFFBID-i));

        //     // recursive call
        //     stratEV = cfres(ngs, 3-player, depth+1, newbidseq, updatePlayer);
        //   }
        //   else
        //   {
        //     // travers over my nodes
        //     for (int i = gs.curbid+1; i <= maxBid; i++)
        //     {
        //       // there is a valid action here
        //       action++;
        //       assert(action < actionshere);

        //       unsigned long long newbidseq = bidseq;
        //       double moveProb = is.curMoveProbs[action];

        //       //CHKPROBNZ(moveProb);

        //       //double newreach1 = (player == 1 ? moveProb*reach1 : reach1);
        //       //double newreach2 = (player == 2 ? moveProb*reach2 : reach2);

        //       GameState ngs = gs;
        //       ngs.prevbid = gs.curbid;
        //       ngs.curbid = i;
        //       ngs.callingPlayer = player;
        //       newbidseq |= (1ULL << (BLUFFBID-i));

        //       double payoff = cfres(ngs, 3-player, depth+1, newbidseq, updatePlayer);

        //       moveEVs[action] = payoff;
        //       stratEV += moveProb*payoff;
        //     }
        //   }

        //   // on my nodes, update the regrets

        //   if (player == updatePlayer)
        //   {
        //     // q(z) = \pi_{-i} is equal to the sampling probabilty, it cancels with the counterfactual term
        //     for (int a = 0; a < actionshere; a++)
        //       is.cfr[a] += (moveEVs[a] - stratEV);
        //   }

        //   // on opponent node, update the average strategy

        //   if (player != updatePlayer)
        //   {
        //     // in stochastically-weighted averaging, divide by likelihood of sampling to here
        //     // also = \pi_{-i}, so they cancel again
        //     for (int a = 0; a < actionshere; a++)
        //       is.totalMoveProbs[a] += is.curMoveProbs[a];
        //   }

        //   // we're always  updating, so save back to the store
        //   iss.put(infosetkey, is, actionshere, 0);

        //   return stratEV;
        // }

        todo!()
    }
}
