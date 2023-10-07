from absl.testing import parameterized
import tensorflow as tf

from open_spiel.python import policy

from open_spiel.python.algorithms import exploitability
import pyspiel

import deep_cfr_tf2

# Get this working for kuhn_poker
# [ ] Add way to track the progress of training
# [ ] Add card encoding to network
# [ ] Tune network size

tf.config.threading.set_intra_op_parallelism_threads(4)
tf.config.threading.set_inter_op_parallelism_threads(4)

game = pyspiel.load_game("kuhn_poker")
deep_cfr_solver = deep_cfr_tf2.DeepCFRSolver(
    game,
    policy_network_layers=(8, 4),
    advantage_network_layers=(4, 2),
    num_iterations=100,
    num_traversals=1000,
    learning_rate=1e-3,
    batch_size_advantage=8,
    batch_size_strategy=8,
    memory_capacity=1e7,
    # save_advantage_networks="./advantage",
    # save_strategy_memories="./strat_mem",
)
deep_cfr_solver.solve()

conv = exploitability.nash_conv(
    game,
    policy.tabular_policy_from_callable(game, deep_cfr_solver.action_probabilities),
)
print("Deep CFR NashConv: {}".format(conv))
