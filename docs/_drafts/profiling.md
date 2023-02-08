<http://www.codeofview.com/fix-rs/2017/01/24/how-to-optimize-rust-programs-on-linux/#fn:3>

valgrind --tool=callgrind --dump-instr=yes --collect-jumps=yes --simulate-cache=yes <path-to-your-executable> [your-executable-program-options]
kcachegrind

perf record -p 23505 -F 100 --call-graph dwarf sleep 120

 perf report --no-inline