---
layout: post
title:  "Counterfactual regret minimization in Rust"
categories: project-log
---

# Context
This post gives a brief overview of two Counter Factual Regret (CFR) algorithms, shows their implementation in Rust, and compares their performance. The code is heavily based on [Marc Lanctot's thesis](http://mlanctot.info/files/papers/PhD_Thesis_MarcLanctot.pdf), [Ziad SALLOUM's post](https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205), and [Max Chiswick's AI Poker Tutorial](https://aipokertutorial.com/the-cfr-algorithm/). They will provide an introduction to some of the concepts mentioned in this post like information states.

All the code for this post can be found [on Github](https://github.com/swpecht/swpecht.github.io/tree/master/projects/liars_poker_bot).

I have implemented both vanilla CFR and CFR with chance sampling (CFRCS). Both algorithms have been evaluated on [Kuhn Poker](https://en.wikipedia.org/wiki/Kuhn_poker) and Bluff ([liar's dice](https://en.wikipedia.org/wiki/Liar%27s_dice)). Overall, for larger games, CFRCS converges to a more optimal strategy faster than CFR.

# CFR algorithm
The CFR algorithm is taken from [section 2.2.2 of Marc Lanctot's thesis](http://mlanctot.info/files/papers/PhD_Thesis_MarcLanctot.pdf):
![CFR algorithm](/assets/cfr-in-rust/vCFR.png)

As someone without an academic background in the subject, this made little sense to me. I understood the high-level concept for CFR -- choose an action that minimizes regret. But I didn't understand the details of how to actually implement it. After doing my own implementation, here a line-by-line breakdown:

--------------
![CFR lines 1-4](/assets/cfr-in-rust/vCFR%201-4.png)
Inititialize all `nodes` with 0 regrest, 0 cumulative probability, and if a move probability needs to be sampled, use a uniform probability over all possible actions.

--------------
![CFR line 5](/assets/cfr-in-rust/vCFR%205.png)
This sections begins the actual CFR function that will be called recursively. It has the following parameters:
* $$h$$: the gamestate, for example in Kuhn Poker it includes the cards that have been dealt and the betting history
* $$i$$: the update player (the player we're calculating the CFR for)
* $$t$$: the depth of the algorithm -- in my implementation, we don't need to track this variable as the gamestate ($$h$$) contains all of the information we need to track istates
* $$\pi_1$$: the probability of player 1 reaching the current gamestate given their play policy
* $$\pi_2$$: the same as above but for player 2

```rust
fn vcfr<T: GameState, N: NodeStore<CFRNode>>(
    &mut self,
    ns: &mut N,
    gs: &T,
    update_player: Player,
    depth: usize,
    reach0: f64,
    reach1: f64,
    chance_reach: f64,
    mut phase: CFRPhase,
) -> f64 {
```

--------------
![CFR line 6-12](/assets/cfr-in-rust/vCFR%206-12.png)
These lines outline 2 special cases for our CFR function.

If the gamestate represents a state where the game is over (i.e. is terminal) return the utility for the update player. In Kuhn Poker, this would be the number of chips won (a positive number) or lost (a negative number).

The second if-statement is for when gamstate is a chance node -- where random action like rolling dice or dealing cards determines what happens instead of player actions. Here we update the reach probability for the acting player ($$\pi_1'$$) as the current reach probability ($$\pi_1$$) times the chance of achieving the random outcome on this gamestate ($$\sigma_c(h,a)$$).

We then call the CFR function on the gamestate with the action applied $$ha$$ and with the updated reach probabilities. We do this for all possible actions and sum the result times the chance of that outcome to get the execected value for the CFR over all chance nodes.

As an example, imagine $$h$$ represents a new game of Kuhn Poker. $$h$$ is a chance node with 3 possible outcomes (Jack, Queen, King) and we're dealing to Player 1. We'd could unroll line 11 to the following:

$$\textbf{return } 1/3 * CFR(h\text{-Jack}, i, 1/3, 1)\\
    + 1/3 * CFR(h\text{-Queen}, i, 1/3, 1) \\
    + 1/3 * CFR(h\text{-King}, i, 1/3, 1)
$$

```rust
if gs.is_terminal() {
    return gs.evaluate()[update_player].into();
}

if gs.is_chance_node() {
    let mut ev = 0.0;

    let actions = &gs.legal_actions();
    for &a in actions {
        let mut ngs = gs.clone();
        ngs.apply_action(a);

        let chance_prob = 1.0 / actions.len() as f64;
        let new_chance_reach = chance_prob * chance_reach;
        ev += chance_prob
            * self.vcfr(
                ns,
                &ngs,
                update_player,
                depth + 1,
                reach0,
                reach1,
                new_chance_reach,
                phase,
            );
    }
    return ev;
}
```

--------------
![CFR line 13-16](/assets/cfr-in-rust/vCFR%2013-16.png)
$$I$$ is the [information set](https://en.wikipedia.org/wiki/Information_set_(game_theory)) for the current gamestate ($$h$$). You can think of $$I$$ as the information available to a given player. For example in Kuhn Poker, my information set could be: `Jack|Bid|Bid` -- I know that I was dealt a Jack, and I know the public actions that have occured a Bid followed by a Bid. But I don't know any of my opponents private information (like what card they were dealt).

$$\sigma^t(I)$$ is the policy at information state $$I$$. You can think of it as a HashMap where the keys are the actions that are legal for the current gamestate and the values are the probability of taking each of those actions.

Here we are updating $$\sigma^t(I)$$ using regret matching ([example code](https://towardsdatascience.com/counterfactual-regret-minimization-ff4204bf4205)).

```rust
let is = gs.istate_key(gs.cur_player());
let mut strat_ev = 0.0;

let actions = gs.legal_actions();
let mut move_evs = ActionVec::new(&actions);

let node = ns
    .get(&is)
    .unwrap_or(Rc::new(RefCell::new(CFRNode::new(gs.legal_actions()))));
let param = match cur_player {
    0 | 2 => reach0,
    1 | 3 => reach1,
    _ => panic!("invalid player"),
};

// Do the regret matching
let move_probs = node.borrow_mut().get_move_prob(param);
```

--------------
![CFR line 17-24](/assets/cfr-in-rust/vCFR%2017-24.png)
We want to calculate the expected value for the current information state. Similarly for what we do for chance nodes, we want to find the expected value of each action and multiply it by the chance we take that actions. We can do this by recursively calling CFR and updating the reach probabilities at each step times the current chance we'll take the action (our policy).


```rust
for &a in &actions {
    let newreach0 = match gs.cur_player() {
        0 | 2 => reach0 * move_probs[a],
        1 | 3 => reach0,
        _ => panic!("invalid player"),
    };

    let newreach1 = match gs.cur_player() {
        0 | 2 => reach1,
        1 | 3 => reach1 * move_probs[a],
        _ => panic!("invalid player"),
    };

    let mut ngs = gs.clone();
    ngs.apply_action(a);
    let payoff = self.vcfr(
        ns,
        &ngs,
        update_player,
        depth + 1,
        newreach0,
        newreach1,
        chance_reach,
        phase,
    );
    move_evs[a] = payoff;
    strat_ev += move_probs[a] * payoff;
}
```

--------------
![CFR line 25-32](/assets/cfr-in-rust/vCFR%2025-32.png)
After we've calculated the expected value for this node and each of the child actions, we want to update the regrets and total move probabilities for future iterations.

The total regret for an action is the probability of reaching this node (accounting for both chance nodes and the opponents policy) time the move expected value less the strategy value (the regret).


```rust
// post-traversals: update the infoset
let (my_reach, opp_reach) = match gs.cur_player() {
    0 | 2 => (reach0, reach1),
    1 | 3 => (reach1, reach0),
    _ => panic!("invalid player"),
};

if cur_player == update_player {
    for &a in &actions {
        let mut n = node.borrow_mut();
        n.regret_sum[a] += (chance_reach * opp_reach) * (move_evs[a] - strat_ev);
        n.total_move_prob[a] += my_reach * n.move_prob[a]
    }

    // save the updates
    ns.insert_node(is, node);
}

return strat_ev;
```

--------------
![CFR line 33-38](/assets/cfr-in-rust/vCFR%2033-38.png)
The final step is to run the CFR algorithm repeatedly for each player until it sufficiently converges.

# Chance sampled CFR
Chance sampled CFR is similar to vanilla CFR except rather than iterating over every possible chance node in a single iteration of CFR, it chooses a single chance node.

Specifically, it requires the two following changes:

**Choose a random chance node rather than iterating**
```rust
if gs.is_chance_node() {
    let a = *actions.choose(&mut self.rng).unwrap();
    let mut ngs = gs.clone();
    ngs.apply_action(a);
    return self.cfrcs(ns, &ngs, update_player, depth + 1, reach0, reach1, phase);
}
```
**Ignore the chance probabilities when updating the regrets**
```rust
if cur_player == update_player {
    for &a in &actions {
        let mut n = node.borrow_mut();
        n.regret_sum[a] += opp_reach * (move_evs[a] - strat_ev); // no chance_prob term
        n.total_move_prob[a] += my_reach * n.move_prob[a]
    }

    // save the updates
    ns.insert_node(is, node);
}
```

# CFR vs CFRCS performance
These minimal changes allows CFRCS to converge faster than vanilla CFR. For example in a game of [bluff](https://en.wikipedia.org/wiki/Liar%27s_poker) with 2 dice for each player, CFRCS converges meaningfully faster than CFR. 
![Bluff22 results](/assets/cfr-in-rust/bluff22%20results.png)

The exploitability is based on the best response algorithm. More details can be found in [Marc Lanctot's thesis](http://mlanctot.info/files/papers/PhD_Thesis_MarcLanctot.pdf).

# Code
All the code for this post can be found [on Github](https://github.com/swpecht/swpecht.github.io/tree/master/projects/liars_poker_bot). But each implementation is repeated below for convience.

**Vanilla CFR**
```rust
pub struct VanillaCFR {
    nodes_touched: usize,
}

impl Algorithm for VanillaCFR {
    fn run<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
    ) {
        self.vcfr(ns, gs, update_player, 0, 1.0, 1.0, 1.0, CFRPhase::Phase1);
    }

    fn nodes_touched(&self) -> usize {
        return self.nodes_touched;
    }
}

impl VanillaCFR {
    fn vcfr<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
        depth: usize,
        reach0: f64,
        reach1: f64,
        chance_reach: f64,
        mut phase: CFRPhase,
    ) -> f64 {
        self.nodes_touched += 1;

        let cur_player = gs.cur_player();
        if gs.is_terminal() {
            return gs.evaluate()[update_player].into();
        }

        if gs.is_chance_node() {
            let mut ev = 0.0;

            let actions = &gs.legal_actions();
            for &a in actions {
                let mut ngs = gs.clone();
                ngs.apply_action(a);

                let chance_prob = 1.0 / actions.len() as f64;
                let new_chance_reach = chance_prob * chance_reach;
                ev += chance_prob
                    * self.vcfr(
                        ns,
                        &ngs,
                        update_player,
                        depth + 1,
                        reach0,
                        reach1,
                        new_chance_reach,
                        phase,
                    );
            }
            return ev;
        }

        // check for cuts  (pruning optimization from Section 2.2.2) of Marc's thesis
        let team = match cur_player {
            0 | 2 => 0,
            1 | 3 => 1,
            _ => panic!("invald player"),
        };
        let update_team = match update_player {
            0 | 2 => 0,
            1 | 3 => 1,
            _ => panic!("invald player"),
        };

        if phase == CFRPhase::Phase1
            && ((team == 0 && update_team == 0 && reach1 <= 0.0)
                || (team == 1 && update_team == 1 && reach0 <= 0.0))
        {
            phase = CFRPhase::Phase2;
        }

        if phase == CFRPhase::Phase2
            && ((team == 0 && update_team == 0 && reach0 <= 0.0)
                || (team == 1 && update_team == 1 && reach1 <= 0.0))
        {
            trace!("pruning cfr tree");
            return 0.0;
        }

        let is = gs.istate_key(gs.cur_player());
        let mut strat_ev = 0.0;

        let actions = gs.legal_actions();

        let mut move_evs = ActionVec::new(&actions);

        let node = ns
            .get(&is)
            .unwrap_or(Rc::new(RefCell::new(CFRNode::new(gs.legal_actions()))));
        let param = match cur_player {
            0 | 2 => reach0,
            1 | 3 => reach1,
            _ => panic!("invalid player"),
        };

        let move_probs = node.borrow_mut().get_move_prob(param);
        // // iterate over the actions
        for &a in &actions {
            let newreach0 = match gs.cur_player() {
                0 | 2 => reach0 * move_probs[a],
                1 | 3 => reach0,
                _ => panic!("invalid player"),
            };

            let newreach1 = match gs.cur_player() {
                0 | 2 => reach1,
                1 | 3 => reach1 * move_probs[a],
                _ => panic!("invalid player"),
            };

            let mut ngs = gs.clone();
            ngs.apply_action(a);
            let payoff = self.vcfr(
                ns,
                &ngs,
                update_player,
                depth + 1,
                newreach0,
                newreach1,
                chance_reach,
                phase,
            );
            move_evs[a] = payoff;
            strat_ev += move_probs[a] * payoff;
        }

        // post-traversals: update the infoset
        let (my_reach, opp_reach) = match gs.cur_player() {
            0 | 2 => (reach0, reach1),
            1 | 3 => (reach1, reach0),
            _ => panic!("invalid player"),
        };
        if phase == CFRPhase::Phase1 && cur_player == update_player {
            for &a in &actions {
                let mut n = node.borrow_mut();
                n.regret_sum[a] += (chance_reach * opp_reach) * (move_evs[a] - strat_ev);
            }
        }

        if phase == CFRPhase::Phase2 && cur_player == update_player {
            for a in actions {
                let mut n = node.borrow_mut();
                n.total_move_prob[a] += my_reach * n.move_prob[a]
            }
        }

        if cur_player == update_player {
            // Todo: move memory to be managed by nodestore -- a get call always returns a node, initialized by the store
            ns.insert_node(is, node);
        }

        return strat_ev;
    }

    pub fn new() -> Self {
        Self { nodes_touched: 0 }
    }
}
```

**CFRCS**
```rust
pub struct CFRCS {
    nodes_touched: usize,
    rng: StdRng,
}

impl Algorithm for CFRCS {
    fn run<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
    ) {
        self.cfrcs(ns, gs, update_player, 0, 1.0, 1.0, CFRPhase::Phase1);
    }

    fn nodes_touched(&self) -> usize {
        return self.nodes_touched;
    }
}

impl CFRCS {
    pub fn new(seed: u64) -> Self {
        Self {
            nodes_touched: 0,
            rng: SeedableRng::seed_from_u64(seed),
        }
    }

    fn cfrcs<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
        depth: usize,
        reach0: f64,
        reach1: f64,
        mut phase: CFRPhase,
    ) -> f64 {
        if self.nodes_touched % 1000000 == 0 {
            debug!("cfr touched {} nodes", self.nodes_touched);
        }
        self.nodes_touched += 1;

        if gs.is_terminal() {
            return gs.evaluate()[update_player].into();
        }

        let cur_player = gs.cur_player();
        let actions = gs.legal_actions();
        if actions.len() == 1 {
            // avoid processing nodes with no choices
            let mut ngs = gs.clone();
            ngs.apply_action(actions[0]);
            return self.cfrcs(ns, &ngs, update_player, depth + 1, reach0, reach1, phase);
        }

        if gs.is_chance_node() {
            let a = *actions.choose(&mut self.rng).unwrap();
            let mut ngs = gs.clone();
            ngs.apply_action(a);
            return self.cfrcs(ns, &ngs, update_player, depth + 1, reach0, reach1, phase);
        }

        // check for cuts  (pruning optimization from Section 2.2.2) of Marc's thesis
        let team = match cur_player {
            0 | 2 => 0,
            1 | 3 => 1,
            _ => panic!("invald player"),
        };
        let update_team = match update_player {
            0 | 2 => 0,
            1 | 3 => 1,
            _ => panic!("invald player"),
        };

        if phase == CFRPhase::Phase1
            && ((team == 0 && update_team == 0 && reach1 <= 0.0)
                || (team == 1 && update_team == 1 && reach0 <= 0.0))
        {
            phase = CFRPhase::Phase2;
        }

        if phase == CFRPhase::Phase2
            && ((team == 0 && update_team == 0 && reach0 <= 0.0)
                || (team == 1 && update_team == 1 && reach1 <= 0.0))
        {
            trace!("pruning cfr tree");
            return 0.0;
        }

        let is = gs.istate_key(gs.cur_player());

        // log the call
        trace!("cfr processing:\t{}", is.to_string());
        trace!("node:\t{}", gs);
        let mut strat_ev = 0.0;

        let mut move_evs = ActionVec::new(&actions);

        let node = ns
            .get(&is)
            .unwrap_or(Rc::new(RefCell::new(CFRNode::new(gs.legal_actions()))));
        let param = match cur_player {
            0 | 2 => reach0,
            1 | 3 => reach1,
            _ => panic!("invalid player"),
        };

        // // iterate over the actions
        let move_probs = node.borrow_mut().get_move_prob(param);
        for &a in &actions {
            let newreach0 = match gs.cur_player() {
                0 | 2 => reach0 * move_probs[a],
                1 | 3 => reach0,
                _ => panic!("invalid player"),
            };

            let newreach1 = match gs.cur_player() {
                0 | 2 => reach1,
                1 | 3 => reach1 * move_probs[a],
                _ => panic!("invalid player"),
            };

            let mut ngs = gs.clone();
            ngs.apply_action(a);
            let payoff = self.cfrcs(
                ns,
                &ngs,
                update_player,
                depth + 1,
                newreach0,
                newreach1,
                phase,
            );
            move_evs[a] = payoff;
            strat_ev += move_probs[a] * payoff;
        }

        let (my_reach, opp_reach) = match gs.cur_player() {
            0 | 2 => (reach0, reach1),
            1 | 3 => (reach1, reach0),
            _ => panic!("invalid player"),
        };

        // // post-traversals: update the infoset
        if phase == CFRPhase::Phase1 && cur_player == update_player {
            for &a in &actions {
                node.borrow_mut().regret_sum[a] += opp_reach * (move_evs[a] - strat_ev);
            }
        }

        if phase == CFRPhase::Phase2 && cur_player == update_player {
            for a in actions {
                let mut n = node.borrow_mut();
                n.total_move_prob[a] += my_reach * n.move_prob[a];
            }
        }

        // todo: figure out if need the explicit updates
        if cur_player == update_player {
            ns.insert_node(is, node);
        }

        return strat_ev;
    }
}
```
