<http://www.codeofview.com/fix-rs/2017/01/24/how-to-optimize-rust-programs-on-linux/#fn:3>

valgrind --tool=callgrind --dump-instr=yes --collect-jumps=yes --simulate-cache=yes <path-to-your-executable> [your-executable-program-options]
kcachegrind

Attach to process:
perf record -p 23505 -F 99 --call-graph dwarf sleep 120

Run executable:
perf record -F 99 --call-graph dwarf ./target/release/liars_poker_bot -v3 --module liars_poker_bot::cfragent



 perf report --no-inline