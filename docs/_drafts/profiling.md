<http://www.codeofview.com/fix-rs/2017/01/24/how-to-optimize-rust-programs-on-linux/#fn:3>

valgrind --tool=callgrind --dump-instr=yes --collect-jumps=yes --simulate-cache=yes <path-to-your-executable> [your-executable-program-options]
kcachegrind

Attach to process:
perf record -p 23505 -F 99 --call-graph dwarf sleep 120

Run executable:
perf record -F 99 --call-graph dwarf,65528 ./target/release/liars_poker_bot -v3 --module liars_poker_bot::cfragent

Need the 65528 to have a large enough stack size


Then hotspot for viewing perf report
hotspot perf.data --debugPaths /usr/lib/debug

 perf report --no-inline

 perf script | stackcollapse-perf.pl | stackcollapse-recursive.pl | c++filt | flamegraph.pl > flame.svg

sudo sysctl -w kernel.perf_event_paranoid=-1


sudo sh -c " echo 0 > /proc/sys/kernel/kptr_restrict"


 https://gist.github.com/dlaehnemann/df31787c41bd50c0fe223df07cf6eb89

 https://github.com/koute/bytehound

 heaptrack_gui heaptrack.liars_poker_bot.15629.zst
 heaptrack {program}