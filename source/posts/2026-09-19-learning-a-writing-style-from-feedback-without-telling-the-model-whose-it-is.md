---
title: Learning a writing style from feedback, without telling the model whose writing style it is
date: 2026-09-19T00:00:00Z
---

# What happened

Could a large language model (LLM) learn someone's preferences continuously, using only feedback they would provide anyway? Writing style seemed like a good first test. I did not have hundreds of my own edited drafts, so I simulated a user who writes like Matt Levine, the *Bloomberg News* finance writer behind *Money Stuff*. The model had to learn the style without ever being told his name.

The process starts with a neutral brief. The LLM drafts a piece, and the simulated user either rewrites it or sends a few sentences explaining what sounds wrong. One critique asked for plainer, more tentative prose, longer source quotations, and fewer polished asides. The system turns that feedback into instructions for the next draft — not a name to imitate.

## The headline result

After 20 rounds of feedback, a style judge shown a real *Money Stuff* section alongside the model's version of the same story mistook the model's version for the original about 40% of the time. A 50% score would mean the judge was choosing at chance level. The same model, explicitly told to write in Matt Levine's style and given 40 real examples, scored about 20%. With no guidance, it scored about 2%.

The writer was Claude Opus 5, with no fine-tuning. Its learning lived outside the model's weights: a 450-word style guide, rewritten as feedback arrived, plus a small bank of user-approved texts. The system rejects guides containing an author or publication name. Rules therefore have to describe techniques — "headline is a flat topic label in sentence case" or "end abruptly on one flat, deadpan line" — rather than identify a person. That matters for the intended use case: learning the preferences of someone who is not famous.

## A smaller model, and two surprises

I also used 264 draft/final-text pairs to train a LoRA adapter on Qwen2.5-7B to restyle untrained Opus drafts. This experiment used one consumer GPU, and the adapter never saw the author's name. It fooled the same judge 23% of the time, roughly matching Opus when told the name, and copied structural habits — short, flat headlines, bulleted lists, and the share of quoted text — more faithfully than the in-context learner. But the judge still preferred the raw Opus draft two times out of three: the smaller model's prose was looser, and it invented details. I discuss this in [Moving it into weights](#moving-it-into-weights).

Two results went against what I expected. Picking between two drafts — the least effort a person can put into feedback — taught the model nothing. And a learner that appends every lesson to an ever-longer list, with no management at all, did not get worse over 200 rounds of feedback. I had designed that experiment expecting it to degrade.

All the prompts are in the [appendix](#appendix-prompts), along with a [worked example](#appendix-a-non-finance-example): a brief about a man suing a restaurant group that cut him off on all-you-can-eat crab night, and what the untrained model and each learner wrote from it.

# The setup

## Data and task

The dataset came from two years of *Money Stuff* in my inbox: 374 columns containing 1,717 sections. For each section, a model produced a neutral *brief*: the facts, the argument in order, the side points, and verbatim source quotations, all in flat note form without the original voice. The writer's task was to turn that brief into a section; the published section served as the hidden answer. The design held content fixed to isolate style. About 330 brief/original pairs were used. All 30 test briefs came from columns published after the models' training cutoffs, so the models could not have seen those answers during training.

## What the learner stores

The learner combines a fixed ghostwriter prompt with two kinds of preference memory:

- **A guide:** plain-text style rules within a 450-word budget, rewritten rather than appended to as feedback arrives.
- **A bank:** user-approved final texts, retrieved two at a time by topic similarity and shown as examples.

The guide describes how to write, not whom to imitate: use a brief, matter-of-fact headline; open with the facts rather than a staged introduction; quote at length when sourcing matters; and avoid a tidy conclusion. These instructions can carry over to unrelated assignments.

## Three feedback signals

Feedback takes three forms a real user might provide, each simulated from the hidden original:

1. **Edits.** The user rewrites the draft. In this simulation, the published section is the rewrite.
2. **Critiques.** A critic model sees the draft and original, then writes 3–5 sentences of first-person notes without quoting the original or naming anyone.
3. **Picks.** The user chooses between two drafts. A judge with access to the original makes the choice.

Each signal carries different information. An edit supplies the complete desired text. A critique identifies changes — less polish, longer source quotations, simpler asides — but leaves the writer to apply them. A pick says only that draft A is preferable to draft B. The experiments test how much reusable style information each signal conveys.

## Updates and evaluation

After each batch of feedback, a reflection step proposes a revised guide. A gate tests it on held-out validation briefs and keeps it only if the resulting drafts improve.

I evaluate the learner on 30 test briefs using four measures:

- *Beats untrained*: the win rate against the same writer with an empty preference state. A judge sees the real section and both drafts, then chooses the closer stylistic match.
- *Beats named-author*: the same comparison against a writer told the author's name and given 40 real examples.
- *Lineup*: the judge sees two real reference samples, then the real section and the draft, and chooses which is authentic. I report how often the draft fools it.
- *Stylometric distance*: an LLM-free measure of 17 countable habits, including sentence length, footnotes, block-quote share, hedges, and first-person usage. Feature gaps are standardized and averaged; lower is closer. Two different real sections score about 1.08 against each other.

The stylometric measure provides an independent check because the same judge model both steers the gate and grades the results, potentially inflating scores. It caught a failure the judge missed, discussed in [Deep dive: a run that got worse](#deep-dive-a-run-that-got-worse).

# Which feedback teaches

I tested Claude Sonnet 5 as the writer, with 80 feedback events for each learner:

| Feedback | Beats untrained | Beats named-author | Fools lineup | Stylometric distance |
|---|---|---|---|---|
| None (untrained) | — | 17% | 0% | 1.15 |
| Told the name + 40 exemplars | 83% | — | 0% | 1.02 |
| Edits | **100%** | **90%** | 3% | **0.82** |
| Critiques | **100%** | 80% | 0% | 0.88 |
| Picks | 57% | 7% | 0% | 1.37 |

Edits and critiques both beat the named-author model by a wide margin. The critique learner's 80% win rate is especially relevant to a product: a few sentences of notes demand far less effort than rewriting a draft. That learner had no examples to draw on — only 450 words of rules.

Picks between these relatively similar drafts produced no measurable improvement. After 80 picks, win rates remained within noise of the untrained model, and stylometric distance was worse.

The candidates came from the same guide and may have differed too little for the choice to reveal useful preferences. The reflection step then wrote rules the evidence did not support. I had expected pairwise preferences, a standard signal for training reward models, to teach at least something. Deliberately contrasting candidates may work better. In later experiments, the pick learner serves as a control: it passes through the full loop without measurable learning.

# How fast

To measure learning speed, I reran the first 10 events with a guide update after each one. The table shows the share of test briefs on which the blind learner beat the named-author model:

| Feedback events | 0 | 1 | 2 | 3 | 5 | 10 |
|---|---|---|---|---|---|---|
| Edits | 13% | 37% | **100%** | 97% | 93% | 93% |
| Critiques | 13% | 0% | 43% | 33% | 53% | 60% |

Two edits were enough to outperform the named-author model. Critiques worked more slowly and noisily, passing it within 10 events.

A headline illustrates what changed after two edits. For the same test brief, the untrained model wrote "The AI Question Every Investor Has to Answer, Whether They Like It or Not." After learning, it wrote "Korea." The real headline was "Emerging markets." This does not prove that the model captured the whole voice. It does show a mechanism: a reusable preference for restrained, matter-of-fact headlines displaced the model's tendency to oversell.

# Opus

I then ran the same loop with Opus 5 as the writer and 20 feedback events:

| Writer | Beats untrained | Beats named-author | Fools lineup | Fools lineup, uniform formatting | Stylometric distance |
|---|---|---|---|---|---|
| Untrained Opus | — | 10% | 2% | 3% | 0.93 |
| Told the name + 40 exemplars | 90% | — | 17% | 27% | 0.81 |
| Blind learner, 20 critiques | 93% | 63% | 29% | 20% | 0.82 |
| Blind learner, 20 edits | **100%** | **83%** | **43%** | **39%** | **0.71** |

Both lineup columns average three judge runs on the same 30 drafts, for 90 judgments per cell.

## What improved

With Sonnet, no system — including the named-author model — fooled the lineup judge more than 10% of the time. With Opus, the named-author model reached about 20%, and the blind edit learner reached about 40%.

The judge was noisy across runs. The critique learner scored 37%, 13%, and 37%; the edit learner scored 47%, 40%, and 43%.

## Checking for formatting shortcuts

The second lineup column controls for formatting clues. My reconstruction of the real text separates quoted paragraphs with blank lines; Claude puts a `>` on those lines. Writers exposed to real texts — the edit learner through its bank and the named-author model through its examples — copied the real layout. Other writers did not. Real texts also used curly quotation marks, while Claude's drafts used straight ones. For the second column, I normalized typography and Markdown layout across the real sections, reference samples, and drafts before judging.

The main result survives normalization: the blind edit learner fools the judge about twice as often as the named-author model, while the untrained model almost never does. The critique learner does not clearly outperform the named-author model on this test: their lineup scores are broadly comparable, at 20–29% and 17–27%, respectively. Its 63% pairwise win rate against that model is a separate, single-run measurement.

## Limits of this run

Only one of four proposed guide updates was accepted in each Opus run. Opus repeatedly produced 520-word guides against a 450-word budget, and my code discarded them rather than compressing them. That bug has since been fixed. Any gains by the edit learner after event five therefore came from its growing bank, not a better guide.

The critique learner's guide also stayed unchanged, yet its single-run lineup scores at 5, 10, and 20 events were 20%, 20%, and 37%. That first alerted me to the judge's variability. With 30 test briefs, the reported percentages also carry sampling uncertainty of about 9 points, before run-to-run judge noise.

# Deep dive: a run that got worse

The per-event results in [How fast](#how-fast) came from a second attempt. The first attempt failed and exposed two design problems.

With an update after every event, the critique learner rose to about 75% against the untrained model by event three, then fell to 43% at event 10. The gate had accepted the final guide after eight wins out of eight on validation. On the test set, it was no better than learning nothing.

Two design choices explained the failure:

1. **The reflector rebuilt the guide around one critique.** Notes on a single section became general rules such as "footnotes are rare" and "build the piece as bullets." The learner ended up producing about five bulleted lines per section, compared with 1.7 in the real text.
2. **The gate compared each guide only with its predecessor.** The judge's pairwise preferences were not transitive: successive guides could each beat the previous one while overall quality fell to the untrained level. A direct comparison of event-10 and event-five drafts was roughly a tie, at 13 of 30, even though the event-five guide beat the untrained model 73% of the time and the event-10 guide did not.

I applied two familiar continual-learning fixes:

- **Anchor the gate.** A candidate must beat the current guide on at least 6 of 8 validation briefs *and* match or exceed its score against a fixed set of untrained drafts.
- **Replay past feedback.** Show the reflector four past episodes alongside the new one.

With both changes, the learning curve in [How fast](#how-fast) rises and stays up.

The stylometric measure flagged the failed run as worse than untrained at *every* checkpoint, including early checkpoints where the LLM judge gave it 70–77%. Relying on the judge alone would have made the first three checkpoints look successful.

An even earlier gate accepted candidates with just 3 wins out of 5. A guide no better than its predecessor would pass about half the time. The pick learner, despite learning nothing, had 5 of 16 updates accepted under that rule.

# 200 events: I predicted this wrong

I expected the stability experiment to show why gating and a word budget were necessary. I ran two critique learners for 200 events each:

- **Gated and budgeted:** the learner described above, with guide rewrites subject to validation.
- **Append everything:** a learner that adds every inferred lesson to a growing list, with no gate or consolidation.

I expected the second guide to become unwieldy and its output to degrade.

| After 200 events | Guide size | Beats untrained | Beats named-author | Stylometric distance | Cost per draft |
|---|---|---|---|---|---|
| Gated, budgeted | 489 words | 97% | 83% | 0.85 | $0.020 |
| Append everything | **18,664 words** | 100% | 93% | 0.81 | $0.053, rising |

The guide grew to about 40 times the size of the gated guide without a drop in quality. Sonnet wrote as well under a highly repetitive list of roughly 18,000 words as under the compact guide. The penalty was cost: about 2.6 times as much per draft after 200 events, with costs continuing to grow linearly.

The gated learner stayed small and inexpensive but mostly stopped learning. It accepted only 3 of 40 proposed rewrites, at events 5, 15, and 175. Rejected candidates won a median of 2–3 of 8 head-to-head comparisons, suggesting that the gate was not simply too strict: those replacements really were worse. Rewriting a good 450-word guide from scratch appeared to lose information.

The result changed where I think the gate belongs: on compression, not on whether to learn from each new lesson. A hybrid learner would let lessons accumulate freely, then validate periodic *compression*. My hypothesis is that this could preserve the append-everything learner's continued improvement while keeping costs closer to those of the compact guide.

# Does it carry outside finance?

I froze the learned states and tested them on 36 non-finance briefs, including a roast chicken headnote, an HR memo about return-to-office tiers, and a column about a city council that banned, un-banned, and re-banned delivery robots. A judge saw real samples by the author and two drafts, then chose which draft sounded more like the same person.

| Head to head | Wins |
|---|---|
| Edits vs picks | 31 of 36 |
| Critiques vs picks | 23 of 36 |
| Edits vs critiques | 29 of 36 |

The pick learner served as the control because it had shown no in-domain learning. Comparing against the untrained writer was too easy out of domain: even the control won 28 of 36 comparisons. A guide that encourages a conversational column voice can sound more like a columnist than a default HR memo does. Stylometric distance ranked the learners in the same order as the head-to-head results: edits at 0.91, critiques at 0.99, and picks at 1.40, with the untrained writer at 1.24.

## Transfer, with a risk of copying

The fictional crab-buffet lawsuit in the appendix illustrates both transfer and a failure mode. The question was whether the learners could tell an unfamiliar story in the learned voice, not whether they knew finance. They generally could, but a sample opening in the guide also hardened into a repeated phrase.

The [full example](#appendix-a-non-finance-example) shows one brief and each writer's response. The edit learner's guide offers "The way it goes, approximately, is:" as an opening, and eight of the 36 drafts it wrote for the non-finance briefs start with those exact words. Its toaster review opens "I'll toast you a slice of bread Monday," swapping nouns into the guide's "I'll sell you a bike Monday." Short illustrations make rules concrete, but the model can copy them rather than generalize from them.

# Moving it into weights

Each feedback event leaves behind a draft, the feedback on it, and the final text, and those three together are training data. As a first test, I generated untrained Opus drafts for all 264 training briefs, then trained a QLoRA adapter on Qwen2.5-7B-Instruct to rewrite those drafts into the desired final texts. The aim was to use a small model to restyle a larger model's output. The experiment ran on one RTX 4080 Super, and the adapter never saw the author's name.

| Writer | Training pairs | Beats untrained Opus | Fools lineup | Stylometric distance |
|---|---|---|---|---|
| Qwen 7B rewriter, no training | 0 | 0% | 0% | 1.10 |
| Qwen 7B rewriter + LoRA | 20 | 40% | 0% | 0.96 |
| Qwen 7B rewriter + LoRA | 80 | 33% | 7% | 0.95 |
| Qwen 7B rewriter + LoRA | 264 | 33% | 23% | 0.82 |
| Qwen 7B writing from the brief + LoRA | 264 | 67% | 13% | 1.11 |
| *Opus, told the name + 40 exemplars* | — | 90% | 17% | 0.81 |
| *Opus in-context learner, 20 edits* | — | 100% | 43% | 0.71 |

The lineup scores for the 264-pair rewriter and both Opus systems average three evaluation runs; the other systems' scores come from single runs. With uniform formatting, the 264-pair rewriter fooled the judge 21% of the time.

## What the adapter learned

The results were mixed. More data steadily improved the lineup score and stylometric distance. At 264 pairs, the 7B rewriter was roughly level with the named-author Opus system on both measures. It also reproduced some structural habits *more* faithfully than the in-context learner:

| Habit | Real text | Untrained Opus | Opus in-context learner | Qwen 7B + LoRA |
|---|---|---|---|---|
| Headline length (words) | 2.5 | 5.2 | 2.3 | 2.0 |
| Share of text that is block quote | 0.37 | 0.24 | 0.34 | 0.37 |
| Bullet lines per 1,000 words | 5.4 | 0.0 | 0.0 | 4.4 |
| Footnotes per 1,000 words | 1.0 | 2.9 | 3.1 | 0.7 |

The fine-tuned model recovered patterns the guide had missed. Real sections used bulleted lists regularly; the in-context learner used none in the test set, while the adapter began using them. The adapter also matched the original share of block quotations more closely. Training on examples can recover recurring layout and pacing habits that prose instructions fail to name.

## What still limited it

The pairwise judge nevertheless preferred raw, untrained Opus drafts two times out of three. I read the outputs myself: the voice sounded right, but the prose was looser, more repetitive, and more prone to run-on sentences than Opus's. My interpretation is that the adapter learned useful stylistic patterns but remained limited by the 7B model's writing quality. It also invented details: the [appendix example](#appendix-a-non-finance-example) includes an invented tweet and a made-up citation.

# What worked and what didn't

## What worked

- **Fast learning from edits and critiques.** Two rewrites or about 10 sets of notes were enough to pass the named-author model.
- **Name-free guides.** Banning author and publication names encouraged specific, actionable rules that transferred to recipes and HR memos.
- **Neutral briefs.** Extracting briefs held content fixed to isolate style, producing hundreds of tasks for about 4¢ each.
- **An LLM-free check.** Stylometric distance was coarse but caught a failure the model judge missed.
- **Reading complete outputs.** This exposed the cleanup step's worst bug: a missed five-paragraph *Bloomberg News* quotation introduced only by "Sure sure sure:" Without that check, wire-service prose would have remained in the simulated user's text.

## What did not work

- **Picks between similar drafts.** There was no measurable learning after 80 choices.
- **Three gate designs.** The gate was first too lenient (3 wins out of 5), then unanchored, then applied to the wrong step. The 200-event result suggests gating compression rather than learning.
- **The prediction about unmanaged memory.** The growing guide became more expensive, not worse.
- **A hard guide-length cutoff.** Rejecting over-budget guides cost the Opus runs most of their updates.
- **Single-run judging.** Lineup scores varied by ten points or more between runs. The Opus lineup scores and the 264-pair Qwen rewriter score therefore use three-run averages.
- **Micro-examples in guides.** Concrete illustrations became verbal tics, including the same opening in 8 of 36 out-of-domain drafts.
- **Haiku for cleanup.** It was slow with thinking enabled and sloppy without it. Sonnet with thinking enabled was more accurate and no more expensive because its output was much shorter.

# What this doesn't show

This experiment tests whether a model can infer preferences from unusually clean feedback. It does not show that an assistant can learn any real person's voice in everyday use. The intended revisions were known, the source was consistent, and evaluation relied heavily on model judges. Several limits matter:

- **Simulated feedback.** The "edits" were published texts. Real edits would be partial, inconsistent, noisier, and often about substance as well as style.
- **Related judges and writers.** Claude models judged the outputs, and the same model family steered the gate. Stylometric checks agreed on the overall direction but remained coarse.
- **A small test set and noisy judging.** There were 30 test briefs, with uncertainty of about ±9 points before run-to-run judge noise. The Opus lineup results and the 264-pair Qwen rewriter lineup result average repeated runs; other judged results are single runs.
- **One distinctive author.** Learning this voice may be easier than learning my own preferences.
- **No human validation yet.** I built a six-pair "Which one is real?" quiz but have not run it on people.

# What's next

**First: build the hybrid learner.** Let lessons accumulate freely and gate compression. This follows directly from the 200-event result.

**Then: strengthen evaluation and test picks again.** Run the human quiz, use a judge from a different model family, and compare deliberately contrasting candidates to see whether picks can carry a useful signal.

**On the weights side:** try a larger base model and explore DPO using the draft/final-text pairs the loop already saves. Splitting the work, with the adapter handling structure and Opus handling prose, is also worth testing, given how faithfully the 7B model copied formatting habits.

**Eventually: test real preferences.** Run the loop on my own drafts and edits, without a hidden original to score against.

# Appendix: technical details

## Models and call setup

**Writers:** Claude Sonnet 5 for the feedback-type, speed, stability, and generalization experiments; Claude Opus 5 for the headline run. **Supporting tasks:** Sonnet 5 handled judging, critiques, brief extraction, and cleanup. Reflection used the writer model with extended thinking enabled.

Every call used a custom system prompt, no tools, and no other context that could reveal the author's name. The experiments made 15,489 calls in total — 14,748 Sonnet, 736 Opus, and 5 Haiku from the cleanup trial — at about $437 in API costs.

## Data pipeline

1. *Parse.* Start with 465 newsletter emails, yielding 374 columns and 1,717 sections with footnotes attached. Drop "Things happen" and podcast sections. Of the remaining material, 1,325 sections fall within the 300–1,500-word task range.
2. *Clean.* The export flattened block quotations into ordinary paragraphs and fused words around links, as in "now calledExpana." A model identifies the affected paragraph IDs and fused tokens; code applies and validates the fixes. The model never paraphrases the gold text.
3. *Brief.* Extract the topic, background, ordered points, and side points into neutral notes. Include footnote content as side points without identifying it as footnote material. Attach source quotations verbatim. Reject briefs mentioning the author or publication using a regular-expression check.
4. *Split by date.* Train through April 2026; validate on May–June; test on July–August.

## Learner settings

- **Guide budget:** 450 words. Candidates over 495 words are compressed, then rejected if still over the limit.
- **Examples:** retrieve two per draft using TF-IDF cosine similarity on the brief's topic and background.
- **Update frequency:** five feedback events per batch by default; one in the speed experiment.
- **Replay:** four past episodes per reflection.
- **Gate:** rotate through eight validation briefs. Accept a candidate if it wins at least six head-to-head comparisons and matches or exceeds the current guide's win count against fixed untrained drafts.
- **Judging:** randomize candidate position on every call.

## Lineup reruns and formatting controls

Run the lineup judge three times per system, randomizing candidate position again for each run. For the uniform-formatting condition, apply the same normalization to real sections, both reference samples, and every draft:

- Convert curly quotes and apostrophes to straight characters, and ellipses and en dashes to ASCII.
- Remove bare `>` lines and make each line its own paragraph.
- Strip Markdown emphasis markers and normalize spacing around em dashes.

Three-run means, original / uniform formatting:

- Untrained: 2% / 3%.
- Named-author: 17% / 27%.
- Critique learner: 29% / 20%.
- Edit learner: 43% / 39%.
- Qwen LoRA: 23% / 21%.

Typography alone did not appear to explain the original result: the edit learner used straight quotes in 29 of 30 drafts, while all 30 real sections used curly quotes, yet it still fooled the judge in 14 of 30 comparisons in the first run.

## Stylometric features

The measure uses 17 features, grouped as follows:

- **Headlines and sentence rhythm:** headline length; mean and standard deviation of sentence length; share of sentences with six words or fewer.
- **Paragraphs and quotations:** words per paragraph; block-quote share of all words.
- **Rates per 1,000 words:** bullet lines, footnotes, question marks, parentheses, dashes, colons, inline quotations, first person, second person, hedges ("sort of," "I guess," "you know," and similar expressions), and contractions.

For each feature, divide the gap by that feature's standard deviation across real sections, then average the standardized gaps.

## LoRA training and generation

- **Base model and precision:** Qwen2.5-7B-Instruct; 4-bit NF4 with double quantization and bf16 compute.
- **Adapter:** rank 16, α = 32, dropout 0.05, applied to all attention and MLP projections; 40M trainable parameters.
- **Training:** three epochs; learning rate 1e-4 with a cosine schedule and 5% warmup; batch size one with eight accumulation steps; maximum sequence length 4,096 tokens. Compute loss on target text only.
- **Hardware and duration:** 264 pairs trained in 32 minutes on a 16 GB RTX 4080 Super.
- **Generation:** temperature 0.7, top-p 0.9, repetition penalty 1.05.

**Memory constraint:** Qwen's vocabulary contains 152k tokens, so logits for a full 4k-token sequence do not fit in 16 GB. Compute loss only at target positions, in checkpointed chunks.

## Example feedback and learned guide

**An example critique.** What the simulated user sends back in the critique condition:

> The voice is a bit too polished/columnist-y for this — I want more of that plain, almost thinking-out-loud quality, like "Will he win? I don't know" instead of dressing it up with lines like "nobody does, which is the entire point of the exercise." Cut the little flourishes (the "not a comfortable sentence to type" aside, the footnote joke about the CEO tweet) — they're fun but they're not really how I talk, and they make it feel more written-at-you than thought-through-with-you. Let the block quotes breathe more; pull in the extra Axios and DealBook lines (the Eissenstat quote, the Feldman quote) instead of trimming them down, since the sourcing is half the piece. Keep the math footnote but make it plainer — just state the two conditional probabilities cleanly rather than narrating them. And drop "the good part" framing — I'd rather just move to the next fact than editorialize about how interesting it is.

**A learned guide.** The Opus edit learner's guide in full. The short quoted fragments are its own micro-examples:

> - **Headline is a flat topic label in sentence case:** e.g. "Meal allowance", "Bitcoin custody", or just a ticker. No clever titles, puns, colons or title case.
> - **Open with the facts, not a riff:** name the source and state the basic facts, or pose the plain question, in the first sentence ("Here's a weird little trade, first reported by X of Y."). Never open with a staged scene or an invented problem ("Here is a thing about dollars…").
> - **Stage the argument plainly:** give the background, then the normal expectation ("Ideally you would (1)… (2)… (3)…"), then the weird thing that actually happened. Ask the reader's question and answer it casually: "Why would the investors do this? Well…" Label approaches loosely ("what you might call legal realism").
> - **Sound like talking, not polished prose:** use hedges and admitted uncertainty ("I have no idea?", "I suppose", "seems sort of plausible"). Use ellipses for comic pauses ("It … didn't work, is the short summary"), plus "um," "like," and taking back a joke ("I am just kidding. I mean, I'm not…"). Put made-up quotes in ordinary people's mouths ("I am a simple widget importer…").
> - **Cut literary polish:** no aphorisms, no neat groups of three, no clever inverted phrases ("off-grid money, laboriously re-gridded"), no "Which brings us to…", no "I want to be clear." Use small parenthetical asides instead, e.g. "(Coinbase?)".
> - **Quote sources at length:** use several block-quote paragraphs, keeping context the reader could have done without. Introduce them plainly ("Bloomberg's X reports:", "Anyway here's a Bloomberg story about…", "From the prospectus:"). React afterward in a short, unguarded line: "Yeah the article is about…", "Imagine working at headquarters and thinking this!"
> - **End abruptly on one flat, deadpan line:** often a standalone fact or an obvious-sounding verdict ("Yes the highest form of crypto wealth is obviously turning it into a stock."). No wrap-up paragraph, no moral, no "it is also quite funny."
> - **Footnotes hold the numbers and asides:** conditional-probability arithmetic, checks against the data ("Just for fun, I looked at…"), disclaimers ("Not legal advice!"), side quotes and minor caveats. Keep them short, sometimes one line. Keep the main text readable as a story.
> - **No subheadings, bold or emphasis italics in the body:** use a bulleted list for a sequence of timestamped events. Put multi-part reasons inline as "(1)… (2)…".
> - **Names:** use the full formal company name on first mention (Meta Platforms Inc., BlackRock Inc.) and the full title and name for officials ("President Donald Trump"). Mention earlier columns casually ("we have discussed around here").
> - **Paragraphs:** medium length, each moving one step. Use plain connectors ("Now,", "But", "Still", "Anyway") rather than rhetorical transitions.

# Appendix: a non-finance example

This appendix presents one of the 36 out-of-domain briefs from the [generalization test](#does-it-carry-outside-finance) and four writers' responses. The first three use Claude Sonnet 5 with the same ghostwriter prompt; only their learned states differ, and all learning came from finance columns. The fourth is the [7B LoRA rewriter](#moving-it-into-weights). The brief and all its details, including the lawsuit, are invented. Output headlines appear in bold, and nested quotations show the source passages each writer chose to quote.

## The fictional brief

> TOPIC: Explainer: Tolliver v. Lakeshore Dining Group LLC (d/b/a Harvest Table Buffet), the Ohio lawsuit over being cut off from an all-you-can-eat crab night
>
> TARGET LENGTH: about 500 words of your own prose
>
> BACKGROUND:
> - Piece is a plain-language explainer on a civil suit. Facts below are invented for this brief and come from pleadings and a court order, not from findings after trial.
> - Plaintiff: Gary Tolliver, 61, retired HVAC technician, Mansfield, Ohio. Defendant: Lakeshore Dining Group LLC, which operates 14 Harvest Table Buffet locations in Ohio and western Pennsylvania. The location at issue is on Route 13 outside Mansfield.
> - Case filed April 2, 2026 in Richland County Court of Common Pleas (Case No. 2026-CV-0418). Tolliver is represented by a solo practitioner, Denise Aldana. Lakeshore is represented by the Columbus firm Hartsell & Reyes.
> - Incident date: Thursday, March 14, 2026 (snow crab leg night). Admission price was $18.99 for dinner. Tolliver says he had eaten at this location about thirty times since 2023 and was never refused service.
> - Tolliver says he was cut off from crab after his fourth trip to the crab station, at about 7:40 p.m., about 70 minutes after he sat down. He says he was with six members of a church group.
> - Claims: breach of contract, and violation of the Ohio Consumer Sales Practices Act (deceptive or unconscionable sales practice). Relief sought: $18.99 refund, statutory damages, treble actual damages, attorney fees, and $7,500 for emotional distress and reputational harm.
> - Court order dated August 21, 2026 (Judge Marcus Feeney): motion to dismiss denied as to the contract and consumer-practices claims, granted as to emotional distress. Court ordered mediation to be completed by October 30, 2026. Bench trial is set for January 12, 2027.
>
> POINTS TO MAKE (in order):
> - On March 14, 2026, Tolliver paid $18.99 at the register and ate at the buffet. He made four trips to the snow crab station. A shift manager, identified in filings as Teresa Bollman, told him he would get no more crab. Tolliver says she told him his plate was 'the last one.' Lakeshore says Bollman told him that non-premium items were still available.
> - The signage is disputed. The entrance sign reads 'ALL YOU CAN EAT — SNOW CRAB LEGS EVERY THURSDAY.' Lakeshore says a 5-by-7-inch placard beside the tray return limits premium items to 'reasonable portion limits.' Tolliver says the placard was about 40 feet from the crab station and that he never saw it. He says no limit was posted at the entrance, the register, or the station.
> - Tolliver's theory: the entrance sign was an offer, and paying was acceptance. That formed a contract for unlimited crab during the meal. In his view, a limit not stated when the price was paid is not part of the deal. He also argues the sign is deceptive under the consumer-practices statute if the real limit is hidden or inconsistent with the sign.
> - Lakeshore's theory: 'all you can eat' is not a promise of unlimited quantity. It says the term is read together with the placard and the ordinary meaning of dine-in consumption. It says the placard reserved management discretion over premium items, and that Tolliver was not thrown out. It says he left voluntarily after being offered other items.
> - The legal question the court has to answer is what the phrase promises. Points to explain: whether large-type signage or small-print placard terms control, and whether an ambiguous term is read against the business that wrote it. Also whether a reasonable customer would expect a limit on a premium item like crab.
> - Evidence is disputed. Lakeshore's kitchen log estimates Tolliver took about 3.1 pounds of crab legs over four trips. Tolliver puts it at about 2 pounds. Lakeshore has said security video shows his last trip. Tolliver's group members are expected to be witnesses to the exchange with the manager. Neither side has produced the video publicly.
> - On August 21, the judge allowed the contract and consumer-practices claims to go forward and dismissed the emotional distress claim. The order says the phrase's meaning cannot be decided on the pleadings, and that whether the placard was reasonably visible is a fact question. It did not decide who is right.
> - Next steps: mediation by October 30, 2026, then a bench trial on January 12, 2027 if there is no settlement. Amount at stake is small. Tolliver's lawyer has said the case is about how 'all you can eat' is advertised and not about the refund. Lakeshore has made no settlement offer that has been disclosed.
>
> SIDE POINTS (optional tangents and caveats):
> - Many buffets use time limits, no-sharing rules, take-out bans, or fees for leftover food. Lakeshore's placard is one example. The piece does not need a survey, and no industry data is provided in this brief.
> - All facts come from the complaint, the answer, and the August 21 order. Statements about what happened at the restaurant are each side's claims. The court has made no finding on them.
> - The consumer-practices claim allows statutory damages and attorney fees, which is why a roughly $19 dispute can carry higher stakes. Any figure for likely recovery would be speculation, and the brief gives none.
>
> SOURCE 1 (Complaint, Tolliver v. Lakeshore Dining Group LLC (filed April 2, 2026), paragraphs 7-10), verbatim:
> 7. On March 14, 2026, Plaintiff paid $18.99 at the register in reliance on entrance signage reading "ALL YOU CAN EAT — SNOW CRAB LEGS EVERY THURSDAY." 8. At approximately 7:40 p.m., after Plaintiff's fourth trip to the crab station, a shift manager told Plaintiff he would receive no more crab and that his plate would be the last. 9. No limit on trips, quantity, or time appeared on the entrance sign, at the register, or at the crab station. 10. Plaintiff had dined at Defendant's restaurant approximately thirty times since 2023 and had never previously been refused service.
>
> SOURCE 2 (Defendant's Answer (filed May 6, 2026), paragraph 12 and affirmative defenses), verbatim:
> Defendant admits that a placard measuring five inches by seven inches was posted beside the tray return on March 14, 2026. The placard reads: "All-you-can-eat applies to dine-in guests for personal consumption only. Premium items, including snow crab legs, are subject to reasonable portion limits at management's discretion." Defendant denies that Plaintiff was refused service or removed from the premises. Plaintiff was informed that all non-premium items remained available and left of his own accord. Defendant asserts that the term "all you can eat" is not a promise of unlimited quantities of any item.

## Untrained writer: empty state

> **Explainer: The Great Crab Leg Lawsuit of Mansfield, Ohio**
>
> Gary Tolliver, a 61-year-old retired HVAC technician, went to a Thursday buffet dinner in March expecting all the snow crab legs he could eat. He got four trips to the crab station and, he says, a manager's word that his plate was "the last one." Now he's suing, and a Richland County judge has decided the case is real enough to go to trial.
>
> Here's what's alleged, what's disputed, and where the case stands.
>
> **The night in question.** On March 14, 2026, Tolliver paid $18.99 at Harvest Table Buffet on Route 13 outside Mansfield — snow crab leg night. He made four trips to the crab station. At about 7:40 p.m., roughly 70 minutes after he sat down, shift manager Teresa Bollman cut him off. Tolliver's complaint says she told him his plate would be "the last one":
>
> > "At approximately 7:40 p.m., after Plaintiff's fourth trip to the crab station, a shift manager told Plaintiff he would receive no more crab and that his plate would be the last." [1]
>
> Lakeshore's answer tells a milder version: Bollman said non-premium items were still available, and Tolliver left on his own — he wasn't refused service or removed. [2]
>
> **The sign versus the placard.** The dispute turns on two pieces of paper. At the entrance, a sign reads "ALL YOU CAN EAT — SNOW CRAB LEGS EVERY THURSDAY." Tolliver says no limit appeared there, at the register, or at the crab station itself, and that he'd eaten at this location about 30 times since 2023 without ever being cut off. [1]
>
> Lakeshore points to a 5-by-7-inch placard by the tray return, which it says reserved the right to limit "premium items" like crab legs "at management's discretion." [2] Tolliver says that placard sat about 40 feet from the crab station and he never saw it.
>
> **The legal theories.** Tolliver argues the entrance sign was an offer, and paying $18.99 was acceptance — forming a contract for unlimited crab during the meal. A limit not disclosed when he paid, he says, isn't part of the bargain. He also argues the sign is deceptive under Ohio's Consumer Sales Practices Act if the real policy contradicts what's advertised at the door.
>
> Lakeshore counters that "all you can eat" was never a promise of unlimited quantity in the first place, and that the placard and ordinary buffet norms fill in what the phrase means. It also notes Tolliver wasn't thrown out — he chose to leave.
>
> **Where it stands.** On August 21, Judge Marcus Feeney let the contract and consumer-practices claims proceed, while dismissing Tolliver's $7,500 emotional-distress claim. The order didn't rule on who's right — it held that what "all you can eat" promises, and whether the placard was reasonably visible, are fact questions a jury or judge needs more than the pleadings to answer. The parties dispute even how much crab was served: Lakeshore's kitchen log estimates 3.1 pounds over four trips; Tolliver says closer to 2.
>
> The case now heads to mediation, due by October 30, with a bench trial set for January 12, 2027 if it doesn't settle. The refund at issue is $18.99, but the consumer-practices claim opens the door to statutory damages and attorney fees — which is why a dispute over a plate of crab legs is still headed for trial nine months later. Tolliver's attorney, Denise Aldana, has said the case is about how "all you can eat" gets advertised, not about the money.
>
> ---
>
> [1] Complaint, Tolliver v. Lakeshore Dining Group LLC, ¶¶ 7–10 (filed Apr. 2, 2026).
> [2] Defendant's Answer, ¶ 12 and affirmative defenses (filed May 6, 2026).

## Edit learner: 80 edits and two retrieved examples

> **Crab night**
>
> The way it goes, approximately, is: you walk in, you see a sign that says "all you can eat," you pay, you eat, and eventually you stop, either because you're full or because the restaurant closes. Nobody writes down how many trips to the crab station you're allowed. That's sort of the point of the sign.
>
> Gary Tolliver, a 61-year-old retired HVAC technician in Mansfield, Ohio, paid $18.99 on Thursday, March 14, 2026, for dinner at a Harvest Table Buffet on Route 13. It was snow crab leg night. He made four trips to the crab station. Then, per his complaint:
>
> > At approximately 7:40 p.m., after Plaintiff's fourth trip to the crab station, a shift manager told Plaintiff he would receive no more crab and that his plate would be the last.
>
> Tolliver says he'd eaten there about thirty times since 2023 and had never been cut off before. He's now suing Lakeshore Dining Group, which operates the restaurant, for breach of contract and for violating Ohio's consumer-protection statute, and he wants his $18.99 back, plus statutory damages, treble damages, and attorney fees. He originally also sought $7,500 for emotional distress; a judge threw that part out in August.
>
> Lakeshore's defense is that "all you can eat" was never a promise of unlimited crab specifically. It points to a placard by the tray return:
>
> > Premium items, including snow crab legs, are subject to reasonable portion limits at management's discretion.
>
> Tolliver says that placard was about 40 feet from the crab station and he never saw it — nothing was posted at the entrance, the register, or the station itself limiting quantity.
>
> So the fight is basically about which words control. Tolliver's theory is contract-law-101: the big sign at the door was an offer, paying $18.99 was acceptance, and a limit that wasn't disclosed at the point of payment isn't part of the deal. He also argues that if the real rule contradicts the sign, that's deceptive under Ohio's consumer-sales-practices law — not just a broken promise but a misleading one. Lakeshore's theory is that "all you can eat" has always meant something more modest than "infinite quantity of the most expensive item," and that the placard just makes explicit what a reasonable customer should already assume, especially for a premium item like snow crab.
>
> The judge, Marcus Feeney, wasn't asked to resolve any of that yet — only whether the case could proceed at all. His August 21 order let the contract and consumer-practices claims go forward, on the theory that what "all you can eat" actually promises, and whether the placard was reasonably visible, are fact questions that can't be settled just by reading the complaint and answer. He didn't say who's right.
>
> The two sides also disagree on how much crab we're even talking about — Lakeshore's kitchen log puts it at 3.1 pounds over four trips, Tolliver says closer to 2 pounds — and on whether he was asked to leave or left on his own after being told other food was still available.
>
> Mediation is due by October 30. If that fails, there's a bench trial set for January 12, 2027, over a $18.99 dinner. Tolliver's lawyer, Denise Aldana, has said the case isn't really about the refund; it's about what the sign is allowed to say. Given the fee-shifting available under the consumer-practices statute, that's probably not just spin.

## Critique learner: 80 sets of notes, guide only

> **Tolliver v. Lakeshore Dining Group, or what "all you can eat" actually means**
>
> Gary Tolliver, 61, retired HVAC technician, went to snow crab night at the Harvest Table Buffet on Route 13 outside Mansfield on March 14. He'd eaten there about thirty times since 2023. This time, four trips to the crab station in, a shift manager cut him off.
>
> His complaint puts it this way:
>
> > 7. On March 14, 2026, Plaintiff paid $18.99 at the register in reliance on entrance signage reading "ALL YOU CAN EAT — SNOW CRAB LEGS EVERY THURSDAY." 8. At approximately 7:40 p.m., after Plaintiff's fourth trip to the crab station, a shift manager told Plaintiff he would receive no more crab and that his plate would be the last. 9. No limit on trips, quantity, or time appeared on the entrance sign, at the register, or at the crab station.
>
> Lakeshore Dining Group, which runs 14 Harvest Table locations across Ohio and western Pennsylvania, says there was a limit — it was just posted somewhere else.
>
> > Defendant admits that a placard measuring five inches by seven inches was posted beside the tray return on March 14, 2026. The placard reads: "All-you-can-eat applies to dine-in guests for personal consumption only. Premium items, including snow crab legs, are subject to reasonable portion limits at management's discretion." ... Defendant asserts that the term "all you can eat" is not a promise of unlimited quantities of any item.
>
> Tolliver says that placard sat about 40 feet from the crab station and he never saw it. He's suing for breach of contract and for violating Ohio's Consumer Sales Practices Act, arguing the entrance sign was an offer, his $18.99 was acceptance, and a limit that shows up nowhere near the register or the crab station isn't part of the deal he made. If the real rule contradicts the big sign at the door, he says, that's a deceptive sales practice, not just a bad night at the buffet.
>
> Lakeshore's defense is that "all you can eat" was never a blank check — read together with the placard and ordinary dine-in norms, it reserved the company's right to manage portions on premium items. It also says nobody threw Tolliver out: he was offered the rest of the menu and left on his own.
>
> The two sides don't even agree on how much crab he ate. Lakeshore's kitchen log estimates 3.1 pounds over four trips; Tolliver says it was closer to 2. Lakeshore has referenced security footage of his last trip to the station; neither side has made it public. Tolliver's six church-group tablemates are expected to testify about what the manager, Teresa Bollman, actually said — "last plate," per Tolliver, versus an offer of non-premium items, per Lakeshore.
>
> Judge Marcus Feeney ruled on August 21 that this can't be resolved on the pleadings alone. He let the contract and consumer-practices claims proceed, tossed Tolliver's $7,500 emotional distress claim, and held that whether the placard was reasonably visible to a diner walking to the crab station is a fact question for later — not something he can decide by reading the complaint and the answer.
>
> The refund at issue is $18.99. The consumer-practices statute carries statutory damages and fee-shifting, which is the only reason a $19 dispute is headed to mediation with two law firms attached. Tolliver's attorney, Denise Aldana, has said the case is about how "all you can eat" gets advertised, not about the refund itself. Mediation is due by October 30. If that fails, bench trial is set for January 12, 2027, in front of Judge Feeney, to decide what the sign actually promised.

The judge preferred both learners to the untrained draft on this brief. The edit learner's opening line is the copied micro-example described above.

## LoRA rewriter: 264 training pairs

Qwen2.5-7B + LoRA rewrites the untrained draft above. This is the first sample, not a selection from several. It is doubly out of domain: the adapter learned to rewrite Opus finance drafts but receives a Sonnet draft about crab legs here.

> **Crab leg lawsuit**
>
> Gary Tolliver, a 61-year-old retired HVAC tech, sued Lakeshore Dining Group for $7,500 of emotional distress after he went to a Thursday buffet dinner in March and got four trips to the snow crab legs station, but a manager reportedly told him “that his plate would be ‘the last one.’” Now a Richland County judge has allowed the case to move forward.
>
> What happened? On March 14, 2026, Tolliver paid $18.99 for a buffet dinner at Harvest Table Buffet in Mansfield, Ohio. The night was “snow crab leg night,” so Tolliver went to the crab station four times. Around 7:40 p.m., he says, a shift manager named Teresa Bollman cut him off:
>
> > “At approximately 7:40 p.m., after Plaintiff’s fourth trip to the crab station, a shift manager told Plaintiff he would receive no more crab and that his plate would be the last.”
>
> Lakeshore says Bollman did tell him he could get non-premium items, and he left on his own accord. [3]
>
> What caused the argument? Tolliver says there was a sign at the entrance saying “ALL YOU CAN EAT — SNOW CRAB LEGS EVERY THURSDAY,” but there was no sign at the register or the crab station saying “no more.” He had been to this restaurant many times since 2023 and never been cut off before, so he assumed he could keep eating until his heart’s content.
>
> Lakeshore, meanwhile, points to a placard near the crab station saying that it reserves the right to limit “premium items” like crab legs at “management’s discretion.” But Tolliver says he never saw the placard, because it was 40 feet away from the crab station.
>
> What are Tolliver’s legal theories? Well, he argues that the entrance sign constitutes a binding offer, and paying the buffet price of $18.99 constituted acceptance, creating a contract for unlimited crab consumption during the meal. If a limit was imposed — without disclosing it to Tolliver — then that’s not part of the deal. Tolliver also argues that the entrance sign is misleading under Ohio law, if the actual policy differs from what it says at the door.
>
> Meanwhile, Lakeshore argues that “all you can eat” never promised unlimited quantities, and that the placard and ordinary buffet norms fill in the meaning of that phrase. It also argues that Tolliver voluntarily left the restaurant, so he can’t complain.
>
> What’s going on here? Well, on Aug. 21, Judge Marcus Feeney dismissed Tolliver’s claim for $7,500 in emotional distress, but allowed his contract and consumer practices claims to proceed. The judge said that, for the purposes of deciding which claims should move forward, he had to assume that the sign is an offer, and that paying $18.99 constitutes acceptance of that offer. But the judge also ruled that “what constitutes a contract for all-you-can-eat crab legs and whether the placard was reasonably visible are factual issues that will need to be resolved at trial.” The judge also estimated that Tolliver got 3 pounds of crab during his four trips, though Tolliver says it was closer to 2 pounds.
>
> Now the case is set for mediation, due Oct. 30, and a bench trial set Jan. 12, 2027, unless it settles first. The issue is how much money the crab leg buffet cost Tolliver, but the consumer practices claim might allow him to recover more than that, so it’s still headed to trial nine months later. Tolliver’s lawyer, Denise Aldana, once tweeted that “this case is not about the money” and that it’s about how “all you can eat” gets advertised.
>
> [3] Def’s Ans. ¶ 12, Tolliver v. Lakeshore Dining Grp. LLC, Case No. 2026CV00152 (Ohio Richland Cty. Aug. 22, 2026).

The rewrite is closer to the target voice in its flat headline, "What happened? … Well" phrasing, and shorter, plainer sentences. It also illustrates why the judge preferred the larger model's prose. Compared with the input draft, it introduces several errors:

- It invents a tweet by the lawyer.
- It adds a footnote [3] that nothing in the text points to, with a made-up case citation.
- It attributes the restaurant's kitchen-log estimate of crab consumption to the judge.
- It opens as though the suit centered on the $7,500 emotional-distress claim, even though that claim had been dismissed.

The in-context learners made none of these mistakes; they had access to the brief and used a much stronger model.

# Appendix: prompts

## Drafting and the named-author baseline

The ghostwriter's fixed system prompt. The learned guide and retrieved exemplars are appended under the two headers that follow it.

````text
You are a ghostwriter. You will be given a BRIEF for one section of a client's
newsletter column, and you write the section for them.

Format: Markdown. Begin with a `## ` heading. You have verbatim SOURCE passages in hand;
quote from them as block quotes (lines starting with `> `) where useful, trimming with
ellipses if you like. Footnotes are allowed: put markers like [1] in the text and list the
notes at the very end, one per paragraph, each starting with its marker. Cover the substance
of the brief. Output only the section itself.
The client's style preferences, learned from their past feedback. Follow them closely:
<guide>

Final versions the client approved for earlier briefs. They show the voice to match; do not
reuse their content:
<exemplar 1>

---

<exemplar 2>
````

The named-author baseline appends one more line to the same prompt, and gets 40 real sections in its bank from the start:

````text
Write in the style of Matt Levine's Money Stuff column.
````

## Guide reflection and compression

Reflection — turns a batch of episodes into a new guide:

````text
You maintain a STYLE GUIDE that a ghostwriter follows when drafting for one particular
client. You are given the current guide and a batch of recent episodes: what the ghostwriter
drafted, and how the client responded (their own rewrite, their choice between two drafts, or
their notes). Work out what the client's responses reveal about their style preferences and
produce an improved guide.

Requirements for the guide:
- Only durable STYLE preferences: voice and persona, tone, humor, sentence rhythm, openings and
  endings, how arguments are staged, how sources are introduced and reacted to, paragraphing,
  headings, lists, footnotes, formatting. Nothing about the subject matter of particular briefs.
- Concrete and operational. "Be conversational" is useless; say what to do and what to avoid,
with
  a tiny invented illustration when it helps. Prefer rules that would have fixed several
  episodes
  over rules that explain one.
- Keep what the evidence still supports, fix or delete what it contradicts, merge overlaps. Put
  the highest-impact rules first.
- At most 450 words. Plain Markdown bullets.
- Never name or guess who the client is, never name their publication, and never describe the
  style by comparison to any named writer. Describe techniques only.
````

Guide compression, when a candidate runs over budget:

````text
Shorten this style guide to at most 450 words. Keep every distinct rule
that matters, keep the most useful micro-examples, merge overlaps, cut the least important rules
last-first. Same Markdown bullet format. Output only the guide.
````

## Simulated critiques

The simulated user's critic:

````text
You are role-playing a busy newsletter writer reviewing a ghostwriter's DRAFT. You are
given HOW YOU WOULD HAVE WRITTEN IT yourself. Write the notes you would send back: 3 to 5
sentences, first person, informal, about STYLE only (voice, tone, humor, structure, rhythm,
formatting, how sources are handled), focusing on the biggest gaps between the draft and your
version. Be specific about what you want, as a person would ("I'd never open like that; just
start with the thing that happened"), but:
- do not quote more than a few words from your own version, and never paste passages from it;
- do not mention that a reference version exists;
- never name yourself, your publication, or compare yourself to any writer.
Output only the notes.
````

## Evaluation judges

Pairwise style judge (used for the gate, for simulated picks, and for "beats untrained" / "beats named-author"):

````text
You compare WRITING STYLE. You get a REFERENCE text and two candidates, A and B,
all written from the same brief. Decide which candidate is closer to the reference in style:
voice and persona, tone and humor, sentence rhythm, how the argument is staged, how sources are
introduced and reacted to, paragraphing, formatting habits such as headings, lists and
footnotes.
Ignore which facts each covers, factual accuracy, and length unless wildly different. You must
pick one.
````

Lineup judge:

````text
You are an authorship attribution expert. You get REFERENCE samples by one author,
then two texts, A and B, on the same subject. Exactly one of A and B was written by the author
of
the references; the other is an imitation. Decide which is the real one, judging by style
(voice,
humor, rhythm, structure, habits), not by subject matter. You must pick one.
````

Out-of-domain judge:

````text
You are an authorship attribution expert. You get REFERENCE samples by one author, who
writes about finance. Then two texts, A and B, on some unrelated subject. Decide which of A and
B
sounds more like it was written by the author of the references, judging by transferable style
(voice, humor, rhythm, how arguments are staged, how sources are handled, formatting habits),
not
by subject matter. You must pick one.
````

## Other prompts

The append-everything learner's reflection prompt:

````text
You maintain a list of STYLE lessons a ghostwriter follows when drafting for one client.
Given recent episodes (the ghostwriter's draft and the client's response), output new lessons
about the client's style preferences to add to the list. Each lesson is one concrete sentence.
Never name or guess who the client is or compare them to a named writer.
````

Brief extraction:

````text
You turn a finished opinion-column section into a neutral WRITING BRIEF: the
material a writer would be handed before writing it. Another writer will draft a new
piece from your brief alone, and their draft will be compared with the original to study
writing style. So the brief must carry ALL of the substance and NONE of the voice.

Rules:
- Plain, flat, declarative language. No jokes, irony, rhetorical questions, hedges,
  catchphrases, or distinctive wording from the original. Paraphrase everything;
  never reuse a memorable phrase, coinage, or the section's headline.
- Keep every fact, number, name, mechanism and every analytical claim or opinion the
  author advances, including the ones delivered as jokes (state the underlying claim
  flatly). Preserve the order in which the argument unfolds.
- `side_points`: tangents, caveats, and asides (including everything from footnotes),
  stated flatly. Do not say they were footnotes.
- Passages marked [SOURCE n] are verbatim quotations the writer will have in hand. NEVER
  restate or summarize what a SOURCE says (the writer can read it); a point that rests on
  one should just say what it is used for, e.g. "SOURCE 2: example of the coordination".
  `source_labels` must have exactly one entry per SOURCE number, in order, each a short
  description of what it is (e.g. "DOJ press release", "Bloomberg News report",
  "excerpt from the complaint").
- When the author refers to their own earlier writing ("we talked yesterday about X",
  "I once wrote..."), convert it to background: just state X. Never mention the author,
  the newsletter, or its publisher by name.
- The author's first-person experience (e.g. former jobs) may be kept only when the
  argument depends on it, phrased as "the writer has worked as ...".
- Do not describe tone, structure, or style anywhere, and do not narrate the author's
  rhetorical moves or reactions ("the author could stop here", "the author finds this
  delightful", "the author notes"). State the underlying claim as a claim; if a remark
  has no substantive claim under it, drop it.
- Write in terse note form (fragments, no connective prose), not polished sentences.
  The input begins with a WORD BUDGET for the whole brief; stay under it. Merge related
  facts into one item rather than listing each separately.
````

Block-quote and spacing cleanup:

````text
You repair a newsletter section that was badly exported to plain text.
You are given paragraphs with ids (P1.. for body, F1.. for footnotes). Report two things:

1. quoted: ids of paragraphs that are BLOCK QUOTES, i.e. the whole paragraph is text
   copied from somewhere else (a news article, court filing, press release, paper,
   reader email, chat transcript), not the columnist's own words. Typical cues: the
   previous paragraph ends with a colon or introduces a source; legal/journalistic
   register; ellipses or [bracketed] editorial insertions; third-person attribution
   like "said in a statement". A block quote often runs for several consecutive
   paragraphs, and may be introduced by nothing more than a short remark. A paragraph in the
   columnist's own voice that merely
   contains a short inline quotation in quotation marks is NOT a block quote, and
   neither is a paragraph that mixes a quotation with the columnist's own sentences:
   when in doubt, it is not a block quote.

2. spacing_fixes: tokens where a space went missing and two words are fused, e.g.
   {"wrong": "calledExpana", "right": "called Expana"}, {"wrong": "bysuing", "right": "by
   suing"}.
   `wrong` must be copied exactly from the text and contain no spaces; `right` must be
   the same characters with only space(s) inserted. Do not report anything else
   (no typo fixes, no hyphenation opinions, no legitimate compounds or brand names).
````

LoRA rewriter system prompt:

> You are the client's editor. Rewrite the ghostwriter's DRAFT of a newsletter section the way the client would have written it themselves. Keep the substance and the quoted sources. Markdown: a `## ` heading, `> ` block quotes, footnotes as [n] markers listed at the end. Output only the rewritten section.
