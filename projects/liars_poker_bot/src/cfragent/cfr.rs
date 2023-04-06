use crate::game::GameState;

pub struct VanillaCFR {
    nodes_touched: usize,
}

impl VanillaCFR {
    // // This is Vanilla CFR. See my thesis, Algorithm 1 (Section 2.2.2)
    // double cfr(GameState & gs, int player, int depth, unsigned long long bidseq,
    //     double reach1, double reach2, double chanceReach, int phase, int updatePlayer)
    // {
    pub fn vcfr<T: GameState>(
        &mut self,
        gs: &T,
        depth: usize,
        reach1: f64,
        reach2: f64,
        chance_reach: f64,
    ) -> f64 {
        // // at terminal node?
        // if (terminal(gs))
        // {
        // return payoff(gs, updatePlayer);
        // }
        let cur_player = gs.cur_player();
        if gs.is_terminal() {
            return gs.evaluate()[cur_player].into();
        }

        // nodesTouched++;
        self.nodes_touched += 1;

        // // Chances nodes at the top of the tree. If p1roll and p2roll not set, we're at a chance node
        // if (gs.p1roll == 0)
        // {
        if gs.is_chance_node() {
            // double EV = 0.0;
            let mut ev = 0.0;

            // for (int i = 1; i <= numChanceOutcomes(1); i++)
            // {
            let actions = &gs.legal_actions();
            for &a in actions {
                // GameState ngs = gs;
                let mut ngs = gs.clone();
                // ngs.p1roll = i;
                ngs.apply_action(a);

                // double newChanceReach = getChanceProb(1,i)*chanceReach;
                let chance_prob = 1.0 / actions.len() as f64;
                let new_chance_reach = chance_prob * chance_reach;
                // EV += getChanceProb(1,i)*cfr(ngs, player, depth+1, bidseq, reach1, reach2, newChanceReach, phase, updatePlayer);
                ev += chance_prob * self.vcfr(&ngs, depth + 1, reach1, reach2, new_chance_reach);
            }
            return ev;
        }

        // // declare the variables
        // Infoset is;
        // unsigned long long infosetkey = 0;
        // double stratEV = 0.0;
        // int action = -1;

        // int maxBid = (gs.curbid == 0 ? BLUFFBID-1 : BLUFFBID);
        // int actionshere = maxBid - gs.curbid;

        // assert(actionshere > 0);
        // double moveEVs[actionshere];
        // for (int i = 0; i < actionshere; i++)
        // moveEVs[i] = 0.0;

        // // get the info set (also set is.curMoveProbs using regret matching)
        // getInfoset(gs, player, bidseq, is, infosetkey, actionshere);

        // // iterate over the actions
        // for (int i = gs.curbid+1; i <= maxBid; i++)
        // {
        // // there is a valid action here
        // action++;
        // assert(action < actionshere);

        // unsigned long long newbidseq = bidseq;
        // double moveProb = is.curMoveProbs[action];
        // double newreach1 = (player == 1 ? moveProb*reach1 : reach1);
        // double newreach2 = (player == 2 ? moveProb*reach2 : reach2);

        // GameState ngs = gs;
        // ngs.prevbid = gs.curbid;
        // ngs.curbid = i;
        // ngs.callingPlayer = player;
        // newbidseq |= (1ULL << (BLUFFBID-i));

        // double payoff = cfr(ngs, 3-player, depth+1, newbidseq, newreach1, newreach2, chanceReach, phase, updatePlayer);

        // moveEVs[action] = payoff;
        // stratEV += moveProb*payoff;
        // }

        // // post-traversals: update the infoset
        // double myreach = (player == 1 ? reach1 : reach2);
        // double oppreach = (player == 1 ? reach2 : reach1);

        // if (phase == 1 && player == updatePlayer) // regrets
        // {
        // for (int a = 0; a < actionshere; a++)
        // {
        // // Multiplying by chanceReach here is important in games that have non-uniform chance outcome
        // // distributions. In Bluff(1,1) it is actually not needed, but in general it is needed (e.g.
        // // in Bluff(2,1)).
        // is.cfr[a] += (chanceReach*oppreach)*(moveEVs[a] - stratEV);
        // }
        // }

        // if (phase >= 1 && player == updatePlayer) // av. strat
        // {
        // for (int a = 0; a < actionshere; a++)
        // {
        // is.totalMoveProbs[a] += myreach*is.curMoveProbs[a];
        // }
        // }

        // // save the infoset back to the store if needed
        // if (player == updatePlayer) {
        // iss.put(infosetkey, is, actionshere, 0);
        // }

        // return stratEV;

        todo!();
    }
}
