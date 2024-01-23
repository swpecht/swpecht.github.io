from open_spiel.python.algorithms.rnad import rnad


def run_solver():
    solver = rnad.RNaDSolver(rnad.RNaDConfig(game_name="kuhn_poker"))
    for _ in range(10):
        solver.step()


run_solver()
