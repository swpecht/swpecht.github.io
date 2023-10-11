import pstats
from pstats import SortKey

p = pstats.Stats("profile")
p.sort_stats(SortKey.CUMULATIVE).print_stats(50)
