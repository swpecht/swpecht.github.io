from absl.testing import parameterized
import tensorflow as tf

from open_spiel.python import policy

from open_spiel.python.algorithms import exploitability
import pyspiel

import deep_cfr_tf2

# TODO: article on performance of expanded cfr:
# "cfr, 3 cards played"   "cfr, 3 cards played"   0.508
# "cfr, 3 cards played"   "pimcts, 50 worlds"     0.572
# "cfr, 3 cards played"   "random"        0.99
# "cfr, 3 cards played"   "cfr, 0 cards played"   0.534
# "pimcts, 50 worlds"     "cfr, 3 cards played"   0.44
# "pimcts, 50 worlds"     "pimcts, 50 worlds"     0.515
# "pimcts, 50 worlds"     "random"        0.99
# "pimcts, 50 worlds"     "cfr, 0 cards played"   0.452
# "random"        "cfr, 3 cards played"   0.005
# "random"        "pimcts, 50 worlds"     0.008
# "random"        "random"        0.491
# "random"        "cfr, 0 cards played"   0.001
# "cfr, 0 cards played"   "cfr, 3 cards played"   0.458

conv_data = []

# Use a random angent as the benchmark


def progress(game, solver):
    solver._learn_strategy_network()
    conv = exploitability.nash_conv(
        game,
        policy.tabular_policy_from_callable(game, solver.action_probabilities),
    )
    solver._reinitialize_policy_network()
    conv_data.append(conv)
    return conv


def train():
    # Get this working for kuhn_poker
    # [*] Add way to track the progress of training
    # [ ] Add exploitability while training -- we're not training the policy network until the end? -- when should this improve?
    # [ ] Add card encoding to network
    # [ ] Tune network size

    game = pyspiel.load_game("kuhn_poker")
    # Parameters from MULTI-AGENT REINFORCEMENT LEARNING IN OPENSPIEL, March 2021
    # https://arxiv.org/abs/2103.00187
    deep_cfr_solver = deep_cfr_tf2.DeepCFRSolver(
        game,
        policy_network_layers=(64, 64, 64),
        advantage_network_layers=(64, 64, 64),
        num_iterations=1,
        num_traversals=15000,
        learning_rate=1e-3,
        batch_size_advantage=2048,  # Cody recommends 32 or 64 as batch size
        batch_size_strategy=2048,  # Cody recommends 32 or 64 as batch size
        memory_capacity=1e7,
        reinitialize_advantage_networks=True,
        policy_network_train_steps=5000,  # defaults
        advantage_network_train_steps=750,  # defaults
        # save_advantage_networks="./advantage",
        # save_strategy_memories="./strat_mem",
        callback=progress,
    )

    deep_cfr_solver.solve()

    conv = exploitability.nash_conv(
        game,
        policy.tabular_policy_from_callable(game, deep_cfr_solver.action_probabilities),
    )
    print("Deep CFR NashConv: {}".format(conv))
    print(conv_data)
    import asciichartpy as acp

    print(acp.plot(conv_data))


train()

# import cProfile

# cProfile.run("train()", "profile")
