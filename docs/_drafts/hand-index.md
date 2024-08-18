Todos:
[*] Confirm that the new iso transformations are actually reversible
  * Need a separate transform for norm and denorm -- since need to know which things to check
[ ] Fix bug with how discards istates are handled, is it easier to insert the discarded card rather than trying to track if they should bbe visible or not?


[ ] Validate istate counts
  [ ] Add way to disable the more advanced istate normalization for debugging

[ ] Optimize training setup
  [ ] Change training setup to report perf a fixed number of times per iteration
  [ ] Only construct a single CFRES object to avoid re-creating the hash function
  [ ] Report the performance after doing a sweep across the entire set of face up cards
[ ] Prep for 4 card played training
  [ ] Use a single hash function trained on 1 face up card with swapping to translate the istates to get the desired index slot
  [ ] Add sharding?

[ ] Fix off by 1 error for depth checker for iterator
[ ] Make infostates have a size of 6
  [ ] Training a baseline euchre agent to validate convergence
  [*] Create indexer class to wrap the phf impelemtation and allow for easier testing -- have it iterate through millions of games -- create an error on
    anything not found to figure out why not being generated
      [*] With 0 played cards, not getting the discard actions included
  [ ] Do some profiling to improve performance
    * Baseline: for 1 card played: 55s
  [ ] Redo the memmap to use a temp file -- then can extend the file dynamicalls
  [ ] Figure out how to surface the max cards played config through the code -- need to know up front to generate the indexing function
  [ ] Create a sharder for the indexer class -- so don't need pass max cards played?
    * Can we just use a single function with a single face up card, and swap from there? 1/6 reduction in run time?
    * Or can cache the hash function in /var? -- only generate once?
    * Probably need to do both here
  [ ] Change the action layout for EAction so can split the u32 into [u8; 4] and then manipulate the suits using a shift instruction on the array
    * See how much this reduces the number of istates
  [ ] Rather than store Istate in mmap vec, store just the array of actions -- saves some of the padding we don't need since only the start of the istates for indexing
  [ ] Switch to indexer based on a single face up card and swap the actual face up card for that one for indexing

https://docs.google.com/spreadsheets/d/10_dIPCG9kCqpJwV5TrhZttz2JGyqd0Ea_4bk53IEfSo/edit#gid=1862664313

Without the suit changing
max cards (IStates the NS as face up): -- incorrect
0: 1_884_344 (1s)
1: 1_884_344 (15s)
2: 13_392_302 (22s) ~x7
3: 235_071_914 (150s) ~x17 (1.7GB)
4: 4_225_304_930 (34 min) ~x18 (304 GB) 

With red suit changing:
0: 229_229
1: 556_171
2: 6_822_091
3: 119_082_943
4:

Create a "shard" function to figure out what shard each thing goes in -- can be a later optimization

Create a slot index function -- converts the played cards into an index

Can be variable sized slots

Then can index the two possible cards within each slot

So the 0 card index gives the overall offset

Then a separate index for the played cards

Should be somewhat straightforward since the played cards are independent

Can use findings from the hand indexer paper to downshift things as needed

Don't need to account for any suit mirroring since can do that separately -- may not be perfect in all cases

Indexing:
[ ] Use the istate validation approach using the re-sample logic
  [ ] Refactor istate resampling to just take in an istate and see if can re-sample it? -- good for debugging purposes as well
  [ ] Can use this to allow defining the max depth calculation in terms of a gamestate
  [ ] Improve istate isomorphism for euchre to also sort the red suits -- see impact

[ ] Investigate way to use phf as the source for the indexer -- can we just generate the phf at startup using the enumeration code? Might be fast enough? especially for euchre? -- could also embed the json in the src file for the known indexers
  [*] Get the kuhn poker test working -- may need to generalize how cards are handled
  [ ] Get the liards dice test working
  [ ] Get the euchre test working
    [ ] Rather than generalizing, is it easier to add a translation layer? Or can we make the underlying representaiton the same without making them the same enums
    [ ] Generalize how cards are handled -- move to new crate so can use across games -- or try to just do the translation manually for now
  [ ] Add a test the the hash is constant between runs
  [ ] What is the interface I want to create between the hasher and iterators?
    * The isomorphic code should be the job of the hasher. Does not require the caller to do the normalizing
    * Should we have separate hashers for each stage of the game? -- then can control the layout of the indexes
  [ ] Add in way to do sharding for the hasher -- could be helpful for euchre
  * This approach doesn't allow for revering the index to a hand, but that's not actually needed for our usecase
  [ ] Create a separate Card crate to use between games -- have it support all cards. Change euchre to support this?
[ ] Switch to Kevin's approach for calculating config sizes
[ ] Add test to indexer for round with 0 cards

[ ] Change isomorphic enumerator to be based on not showing a lower hand -- need to change the deal enumerator to actually be lowest to highest
  * Can we use the index group function? -- pretty simple, and instead of it being rounds, it could be by suit?
[*] Create way to define what suit transformations are allowed -- enables supporting euchre
  * How to adapt this for euchre?
[ ] Speed up the iso hand generation -- change the iteration order so that we wrap at suit ends rather than end of deal
[ ] Add more testing for the poker version of indexing -- with actual hands
  [ ] Fix off by 1 errors on correcting for the index offsets
  * Switch to storing the first index for the config -- should fix off by 1 and size errors
[ ] Look at training bot to calculate actual number of isomorphic euchre deals

[ ] For indexing, do we do 1 suit as 12 cards and 2 other suits as 6 -- the first 2 suits get merged into 1 suit


[ ] Create way to define un-related rounds and non-card actions -- with transforms?


# enumerate cardset

Let N be num cards in set

All first suit
(16 choose N)

One second
(16 choose N-1) * 16

Two second
(16 choose N-2) * (16 choose 2)

# Index cardset


Let $[x^k]$ be the coefficient operator -- it represents the coefficient of $x^k$ in the series

Need to develop $f(C, N)
\rightarrow \text{Size}$

Where:
* $S$: number of suits
* $C=[c_0, c_1, \ldots c_S]$: number of cards used in this round from suit $i$
* $N=[n_0, n_1, \ldots, n_i]$: number of cards remaining in suit $i$ for this round

# Approach
The number of combinations is independent for suits with a different number of cards drawn
$\prod_{x \in C} |[c \in C|c =x]|$

$[x_1, y_1][x_2, y_2]\ldots$ for number configuration with $y_1$ suits of $x_1$ cards, $y_2$ suits of $x_2$ cards and so on.

Where:
* |x| is the number of combinations of |x|

$[x^k][(\sum_{i=0}^{S}x^i)^{\binom{n_min}{c}}*(1+x)^{\binom{n_next}{c}-\binom{n_min}{c}}]$

# Scratch


The generator undercounts because it doesn't differentiate if the 9 and 10 are different suits: https://math.stackexchange.com/questions/4817349/calculating-number-of-variants-of-variants-with-a-generating-function

How to fix it?


https://math.berkeley.edu/~mhaiman/math172-spring10/exponential.pdf

https://math.stackexchange.com/questions/2662326/constrained-combinatorial-question-using-generating-functions

https://math.stackexchange.com/questions/2662326/constrained-combinatorial-question-using-generating-functions

Define $F(a, n)$ as coefficient of $x^n$ when $a$ is expanded


https://math.stackexchange.com/questions/4813416/calculating-the-number-of-non-equivalent-hands-for-a-given-suit-configuration
55 for N=5 [2, 2] using he approach in the answer $\binom{\binom{5}{2}+2-1}{2}=55$

19

(1+x)^5*(1+y)^5
y^2 x^2 = 100


SeriesCoefficient[(1+x)^5, [x, 0, 2]]

https://en.wikipedia.org/wiki/Generating_function#Exponential_generating_function_(EGF)



Pre-flop: 169
* $[1][1]$: $F(1+x+x^2)^{13}, 2) = 91$
* $[2]$: $F((1+x)^{13}, 2)=78$

River: 1,286,792
* $[2, 3]$: $F((1+x)^{13}, 2)*F((1+x)^{11}, 3)=12870$
* $[2, 2], [0, 1]$: $F((1+x)^{13}, 2)*F((1+x+x^2)^11*(1+x)^2, 3)$ = Is this right? 78 * 418 = 32604, should be 55770
* $[2, 1], [0, 2]$
* $[2, 1], [0, 1], [0, 1]$
* $[2, 0], [0, 3]$
* $[2, 0], [0, 2], [0, 1]$
* $[2, 0], [0, 1], [0, 1], [0, 1]$
* $[1, 3], [1, 0]$
* $[1, 2], [1, 1]$
* $[1, 2], [1, 0], [0, 1]$
* $[1, 1], [1, 1], [0, 1]$
* $[1, 1], [1, 0], [0, 2]$
* $[1, 1], [1, 0], [0, 1], [0, 1]$
* $[1, 0], [1, 0], [0, 3]$
* $[1, 0], [1, 0], [0, 2], [0, 1]$

