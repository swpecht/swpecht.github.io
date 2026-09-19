---
title: Learning a writing style from feedback, without telling the model whose it is
date: 2026-09-19T00:00:00Z
---

# What happened

I wanted to know whether a model can learn a person's preferences continuously, just from the feedback they'd give anyway. Writing style seemed like a good first test, and since I don't have a few hundred of my own edited drafts, I simulated the user: the simulated user writes like Matt Levine, and the model has to learn that style without ever being told his name.

The short version: after 20 rounds of feedback, a style judge shown the real *Money Stuff* section next to the model's version of the same story picked the model's as the real one about 40% of the time. At 50% the judge can't tell the model's version from the real one at all. The same model told outright to "write in the style of Matt Levine's Money Stuff column," and given 40 real examples to draw on, scored about 20%. With no guidance it scored about 2%.

That's Claude Opus 5 as the writer. Everything it learned is stored as text outside the model's weights: a 450-word style guide that it rewrites as feedback comes in, plus a small memory of texts the user approved. The system rejects any guide that contains an author or publication name, so the guide has to describe techniques — "headline is a flat topic label in sentence case," "end abruptly on one flat, deadpan line" — instead of naming a person. The real use case is a user who isn't famous, where naming an author isn't an option.

Two results went against what I expected. Picking between two drafts, the cheapest feedback a person can give, taught the model nothing. And a learner that appends every lesson to an ever-longer list, with no management at all, did not get worse over 200 rounds of feedback. I had designed that experiment expecting it to degrade.

*Update, September 19: the first version of this post gave those figures as 47%, 17% and 0%, from a single run of the judge. Re-running the judge showed that single runs move by ten points or more, so the post now reports averages over three runs. The ordering didn't change. Details are in the [Opus section](#opus).*

All of the prompts are in the [appendix](#appendix-prompts), along with a [worked example](#appendix-a-non-finance-example): a brief about a man suing a buffet, and what the untrained model and each learner wrote from it.

# The setup

**Data.** Two years of *Money Stuff* from my inbox: 374 columns, 1,717 sections. For each section, a model writes a neutral *brief* — the facts, the argument in order, the side points, and the quoted sources word for word — in flat note form with none of the voice. The task is then "write this brief up," and the real section is the hidden answer. Content is held fixed, so style is the only thing that varies. About 330 brief/original pairs ended up in use. The 30 test briefs all come from columns published after the models' training cutoff, so the model can't have seen the answers.

**The learner** is a fixed ghostwriter prompt plus a preference state with two tiers:

- a **guide**: plain-text style rules under a 450-word budget, rewritten (not appended to) as feedback arrives
- a **bank**: final texts the user approved, retrieved two at a time by topic similarity and shown as exemplars

**Feedback** comes in the three forms a real user would give, all simulated from the hidden original:

1. **Edits.** The user rewrites the draft. (The "rewrite" is the real section.)
2. **Critiques.** The user sends 3–5 sentences of notes. A critic model sees the draft and the original and writes notes in the first person, without quoting the original or naming anyone.
3. **Picks.** The user is shown two drafts and picks one. A judge that can see the original does the picking.

**Updates.** After each batch of feedback, a reflection step proposes a new guide. A gate tests it on held-out validation briefs and keeps it only if drafts written under it are better.

**Scoring**, on the 30 test briefs:

- *Beats untrained*: a judge sees the real section and two drafts and says which is closer in style. Win rate against the same model with an empty state.
- *Beats named-author*: same, against the model that was told the name and given 40 real exemplars.
- *Lineup*: the judge gets two real samples by the author, then the real section and the draft, and has to say which is real. I report how often it's fooled.
- *Stylometric distance*: no LLM involved. Seventeen countable habits (sentence length, footnotes, block-quote share, hedges, first person, and so on), z-scored and averaged. Lower is closer. Two different real sections score about 1.08 against each other.

That last one exists because the same judge model steers the gate and grades the results, which could inflate the scores. It caught one failure that the judge missed, described below.

# Which feedback teaches

Claude Sonnet 5 as the writer, 80 feedback events each:

| Feedback | Beats untrained | Beats named-author | Fools lineup | Stylometric distance |
|---|---|---|---|---|
| None (untrained) | — | 17% | 0% | 1.15 |
| Told the name + 40 exemplars | 83% | — | 0% | 1.02 |
| Edits | **100%** | **90%** | 3% | **0.82** |
| Critiques | **100%** | 80% | 0% | 0.88 |
| Picks | 57% | 7% | 0% | 1.37 |

Edits and critiques both beat the model that was told the author's name, by a wide margin. Critiques getting to 80% is the result I care about most for a product: a few sentences of notes cost the user almost nothing compared with rewriting a draft, and that learner has no exemplars at all — just 450 words of rules.

Picks taught nothing. After 80 of them the learner's win rates are within noise of the untrained model's, and its stylometric score is worse. The two drafts come from the same guide, so they differ too little for the choice to carry information, and the reflection step writes rules the evidence doesn't support. Pairwise preference is the standard signal for training reward models, so I assumed it would at least do *something* here. It probably needs deliberately contrasting candidates. In the later experiments I use the pick learner as a control: it goes through the whole loop and learns nothing.

# How fast

I reran the first ten events with a guide update after every single one. Share of test briefs where the blind learner beats the named-author model:

| Feedback events | 0 | 1 | 2 | 3 | 5 | 10 |
|---|---|---|---|---|---|---|
| Edits | 13% | 37% | **100%** | 97% | 93% | 93% |
| Critiques | 13% | 0% | 43% | 33% | 53% | 60% |

Two edits were enough to pass the named-author model. Critiques are slower and noisier, and pass it within ten.

One example of what two edits does. For the same test brief, the untrained model's headline was "The AI Question Every Investor Has to Answer, Whether They Like It or Not." After learning, it was "Korea." The real one is "Emerging markets."

# Opus

Same loop, Opus 5 as the writer, 20 events:

| Writer | Beats untrained | Beats named-author | Fools lineup | Fools lineup, uniform formatting | Stylometric distance |
|---|---|---|---|---|---|
| Untrained Opus | — | 10% | 2% | 3% | 0.93 |
| Told the name + 40 exemplars | 90% | — | 17% | 27% | 0.81 |
| Blind learner, 20 critiques | 93% | 63% | 29% | 20% | 0.82 |
| Blind learner, 20 edits | **100%** | **83%** | **43%** | **39%** | **0.71** |

Both lineup columns are averages over three runs of the judge on the same 30 drafts, 90 judgments per cell.

With Sonnet, nothing ever fooled the lineup judge more than 10% of the time, the named-author model included. With Opus the named-author model gets to about 20% and the blind edit learner to about 40%.

The lineup judge is noisy. Single runs for the critique learner came out at 37%, 13% and 37%; for the edit learner, 47%, 40% and 43%. The first version of this post reported the first run of each.

The second lineup column exists because I found a formatting difference the judge could have been using. My reconstruction of the real text separates quoted paragraphs with a blank line; Claude writes `>` on that line. The writers that had seen real text (the edit learner through its bank, the named-author model through its exemplars) copy the real layout, and the others don't. The real text also uses curly quotes where Claude's drafts use straight ones. For the second column, typography and Markdown layout are made identical across the real sections, the reference samples and every draft before judging.

What holds up in both columns: the blind edit learner fools the judge about twice as often as the model that was told the author's name, and the untrained model almost never does. What doesn't hold up is any claim that the critique learner beats the named-author model on this test. They are about level, at 20–29% against 17–27%. The critique learner's 63% pairwise win rate against the named-author model is a separate measurement, from a single run.

Two caveats on that table. Only one of four proposed guide updates was accepted in each Opus run, because Opus kept writing 520-word guides against a 450-word budget and my code threw them away instead of compressing them (fixed since). So whatever the edit learner gained after event five came from the bank growing, not the guide improving. The critique learner's single-run lineup figures at 5, 10 and 20 events were 20%, 20% and 37% with an unchanged guide, which is what first suggested the judge was noisy. With 30 test briefs, every percentage in this post has a standard error of about 9 points.

# Deep dive: a run that got worse

The per-event rerun above is the second attempt. The first one failed, and the failure exposed two problems in the design.

With updates after every event, the critique learner climbed to about 75% against the untrained model by event three — then fell back to 43% at event ten. The gate had accepted that final guide with 8 wins out of 8 on validation. On the test set it was no better than having learned nothing.

Two things were going on:

1. **The reflector rewrote the whole guide around one critique at a time.** A single set of notes on a single section produced rules like "footnotes are rare" and "build the piece as bullets." The learner's drafts ended up with about five bullet lines per section against 1.7 in the real text.
2. **The gate only compared each candidate with the guide before it.** The judge's pairwise preferences aren't transitive, so a chain of guides can each beat its predecessor while the absolute quality falls back to the untrained level. I re-judged the event-ten drafts head to head against the event-five drafts to check: roughly a tie, 13 of 30, even though the event-five guide had beaten untrained 73% of the time and the event-ten guide couldn't.

The fixes are both old ideas from continual learning. **Anchor the gate:** a candidate must beat the current guide on at least 6 of 8 validation briefs *and* score at least as well as the current guide against a fixed set of untrained drafts. **Replay:** show the reflector four past episodes alongside the new one. With both, the curve in the previous section rises and stays up.

The stylometric measure had flagged this run as worse than untrained at *every* checkpoint, including the early ones where the LLM judge was still giving it 70–77%. If I'd only had the judge I would have believed the first three checkpoints.

An earlier version of the gate was worse still: accept on 3 wins out of 5. A guide that's no better than the current one passes that about half the time. The pick learner, which learned nothing, had 5 of 16 updates accepted.

# 200 events: I predicted this wrong

The stability experiment was meant to show why the gate and the word budget are needed. Two critique learners, 200 events each. One is gated and budgeted as above. The other appends every lesson it thinks of to a list that only grows, with no gate and no consolidation. I expected the second one's guide to grow very large and its output to degrade.

| After 200 events | Guide size | Beats untrained | Beats named-author | Stylometric distance | Cost per draft |
|---|---|---|---|---|---|
| Gated, budgeted | 489 words | 97% | 83% | 0.85 | $0.020 |
| Append everything | **18,664 words** | 100% | 93% | 0.81 | $0.053, rising |

The guide grew to about 40 times the size of the gated one, and quality did not drop. Sonnet writes as well under an 18,000-word, highly repetitive list of lessons as under a 450-word guide. The cost is money: 2.6 times as much per draft after 200 events, and it keeps growing linearly.

Meanwhile the gated learner stayed small and cheap and mostly stopped learning. It accepted 3 of 40 proposed rewrites, at events 5, 15 and 175. The rejected candidates won a median of 2–3 of 8 head-to-heads, so the gate wasn't being too strict — those rewrites really were worse than what they'd replace. Rewriting a good 450-word guide from scratch loses information.

So the design I built is the wrong one. The obvious fix is to let lessons accumulate freely and put the gate on the periodic *compression* step instead. That should keep the append-everything learner's continued improvement at the gated learner's constant cost.

# Does it carry outside finance?

I froze the learned states and had them write 36 briefs that have nothing to do with finance: a roast chicken headnote, an HR memo about return-to-office tiers, a column about a city council that banned, un-banned and re-banned delivery robots. A judge sees real samples by the author and two drafts and says which sounds more like the same person.

| Head to head | Wins |
|---|---|
| Edits vs picks | 31 of 36 |
| Critiques vs picks | 23 of 36 |
| Edits vs critiques | 29 of 36 |

The pick learner is the control, since it learned nothing in-domain. I had to use it because "beats the untrained model" turned out to be too easy a test out of domain — even the control won that 28 of 36. Any guide that pushes toward a conversational column voice sounds more like a columnist than a default HR memo does. The stylometric measure orders the three learners the same way as the head-to-heads (0.91, 0.99, 1.40, with untrained at 1.24).

There's a full example in the [appendix](#appendix-a-non-finance-example): one brief and what each writer produced from it. It also shows a flaw. The edit learner's guide contains the micro-example "The way it goes, approximately, is:" as one way to open a piece, and 8 of its 36 out-of-domain drafts open with exactly those words. Its toaster review starts "I'll toast you a slice of bread Monday," which is the guide's "I'll sell you a bike Monday" with the nouns swapped. Short illustrations make the rules concrete, and the model copies them word for word.

# Moving it into weights

The loop saves a (draft, feedback, final text) triple for every event, which is training data. As a first pass I generated untrained Opus drafts for all 264 training briefs and trained a QLoRA adapter on Qwen2.5-7B-Instruct to rewrite an untrained Opus draft into the text the user wanted . The idea is a small model that restyles the large model's output. Training ran overnight on one RTX 4080 Super. The adapter never sees the author's name either.

| Writer | Training pairs | Beats untrained Opus | Fools lineup | Stylometric distance |
|---|---|---|---|---|
| Qwen 7B rewriter, no training | 0 | 0% | 0% | 1.10 |
| Qwen 7B rewriter + LoRA | 20 | 40% | 0% | 0.96 |
| Qwen 7B rewriter + LoRA | 80 | 33% | 7% | 0.95 |
| Qwen 7B rewriter + LoRA | 264 | 33% | 23% | 0.82 |
| Qwen 7B writing from the brief + LoRA | 264 | 67% | 13% | 1.11 |
| *Opus, told the name + 40 exemplars* | — | 90% | 17% | 0.81 |
| *Opus in-context learner, 20 edits* | — | 100% | 43% | 0.71 |

Lineup figures for the 264-pair rewriter and the two Opus rows are three-run averages; the other rows are single runs. With formatting made uniform the 264-pair rewriter scores 21%.

The results are mixed. More data steadily helps on the lineup test and the stylometric measure, and at 264 pairs a 7B model is roughly level with Opus-told-the-name on both. It also copies structural habits *better* than the in-context learner:

| Habit | Real text | Untrained Opus | Opus in-context learner | Qwen 7B + LoRA |
|---|---|---|---|---|
| Headline length (words) | 2.5 | 5.2 | 2.3 | 2.0 |
| Share of text that is block quote | 0.37 | 0.24 | 0.34 | 0.37 |
| Bullet lines per 1,000 words | 5.4 | 0.0 | 0.0 | 4.4 |
| Footnotes per 1,000 words | 1.0 | 2.9 | 3.1 | 0.7 |

The Opus in-context learner never wrote a single bullet list in 30 sections, even though the real text uses them constantly. The 7B adapter picked that up from examples without anyone writing it down as a rule.

But the pairwise judge still preferred the raw untrained Opus draft two times in three. I read the outputs: the voice is right and the prose is looser and more repetitive than Opus's, with run-on sentences. So the adapter learned the style, and what limits it is the 7B model's writing quality. The [appendix example](#appendix-a-non-finance-example) ends with one of its rewrites, which includes an invented tweet and a made-up citation.

# What worked and what didn't

What worked:

- Blind learning from edits or critiques, fast. Two rewrites or about ten sets of notes to pass a model that was told the answer.
- Banning names from the guide. The rules it wrote are specific and operational, and they transfer to recipes and HR memos.
- Holding content fixed with extracted briefs. Hundreds of tasks for about 4¢ each, and style is the only variable.
- A metric with no LLM in it. It's coarse, but it caught a failure the LLM judge missed.
- Reading outputs end to end. That's how the cleanup step's worst bug turned up: it missed a five-paragraph Bloomberg News quotation introduced only by "Sure sure sure:", which would have left wire-service prose in the "user's" text.

What didn't:

- Pairwise picks between similar drafts. No measurable learning after 80 picks.
- My gate, three times: too lenient (3 of 5), then not anchored, then applied to the wrong step (it should gate compression, not learning).
- My prediction about unmanaged memory. It gets expensive, not worse.
- The 450-word budget as a hard reject. It cost the Opus runs most of their updates.
- Reporting one run of a noisy judge. The first version of this post had 47% in its second paragraph; the three-run average is 43%, and 39% with formatting made uniform.
- Micro-examples in the guide. They make rules concrete and then show up verbatim as tics: the same opening line in 8 of 36 out-of-domain drafts.
- Haiku for cleanup: slow with thinking on, sloppy with it off. Sonnet with thinking on was both more accurate and no more expensive, because its output is much shorter.

# What this doesn't show

- The user is simulated, and its "edits" are the real published text. A real person's edits would be partial, inconsistent and noisier.
- The judges are Claude models, and the same family steers the gate. The stylometric measure agrees on direction everywhere but is coarse.
- 30 test briefs. About ±9 points on everything, before the judge's own run-to-run noise. Only the Opus lineup figures are averaged over repeated runs; every other judged number in this post is a single run.
- One author. A distinctive one, which probably makes this easier than learning my preferences would be.
- The lineup judge is a model. I built a six-pair "which one is real?" quiz for humans and haven't run it on anyone yet.

# What's next

The hybrid learner: append lessons freely, gate the compression. That follows directly from the 200-event result and is the first thing I'll build.

Then the human quiz, a judge from a different model family, and picks with deliberately contrasting candidates to see whether picks can carry any signal.

On the weights side, a bigger base model is the obvious next step, and the loop already saves a draft-versus-final pair for every event, which is the input DPO needs. The adapter-for-structure, Opus-for-prose split is also worth a try given how well the 7B model copied formatting habits.

Eventually I want to run this on my own drafts and my own edits, where there is no hidden original to score against.

# Appendix: technical details

**Models.** Writer: Claude Sonnet 5 for the feedback-type, speed, stability and generalization experiments; Claude Opus 5 for the headline run. Judge, critic, brief extraction and cleanup: Sonnet 5. Reflection uses the writer model with extended thinking on. Every call is a bare model call: custom system prompt, no tools, and nothing else in context that could leak the author's name. 15,489 calls in total — 14,748 Sonnet, 736 Opus — about $437 at API prices.

**Pipeline.**

1. *Parse.* 465 newsletter emails → 374 columns → 1,717 sections with footnotes attached. "Things happen" and podcast sections dropped. 1,325 sections fall in the 300–1,500-word range used as tasks.
2. *Clean.* The export had flattened block quotes into ordinary paragraphs and fused words around links ("now calledExpana"). A model only *points at* problems — which paragraph ids are block quotes, which tokens are fused — and code applies and validates the fixes. No model ever paraphrases the gold text.
3. *Brief.* A model extracts topic, background, points in order, and side points (footnote content goes here without saying it was a footnote), in note form. Quoted sources are attached verbatim. Briefs mentioning the author or publication are rejected by a regex lint.
4. *Split by date.* Train through April 2026, validate May–June, test July–August.

**Learner settings.** Guide budget 450 words (candidates over 495 are compressed, then rejected if still over). Two exemplars retrieved per draft by TF-IDF cosine on the brief's topic and background. Default batch of 5 events per update; the speed experiment uses 1. Replay: 4 past episodes per reflection. Gate: 8 validation briefs on rotation, accept if the candidate wins ≥ 6 head-to-heads and its win count against fixed untrained drafts is at least the current guide's. Judge position is randomized on every call.

**Lineup re-runs.** Three runs of the lineup judge per system, with judge position re-randomized each run. For the uniform-formatting column every text (real sections, the two reference samples, all drafts) goes through the same normalizer first: curly quotes and apostrophes to straight, ellipsis and en-dash characters to ASCII, bare `>` lines removed, every line made its own paragraph, Markdown emphasis markers stripped, and spacing around em dashes made uniform. Three-run means, original / uniform: untrained 2% / 3%, named-author 17% / 27%, critique learner 29% / 20%, edit learner 43% / 39%, Qwen LoRA 23% / 21%. One check that typography alone wasn't driving the original result: the edit learner used straight quotes in 29 of 30 drafts against curly quotes in all 30 real sections, and still fooled the judge in 14 of 30 in the first run.

**Stylometric features** (17): headline length; sentence-length mean and standard deviation; share of sentences ≤ 6 words; words per paragraph; block-quote share of all words; and per-1,000-word rates of bullet lines, footnotes, question marks, parentheses, dashes, colons, inline quotations, first person, second person, hedges ("sort of," "I guess," "you know," …) and contractions. Each feature's gap is divided by that feature's standard deviation across real sections, then averaged.

**LoRA.** Qwen2.5-7B-Instruct, 4-bit NF4 with double quantization, bf16 compute. LoRA r=16, α=32, dropout 0.05 on all attention and MLP projections (40M trainable parameters). 3 epochs, lr 1e-4 cosine with 5% warmup, batch 1 × 8 accumulation, max 4,096 tokens, loss on the target text only. 264 pairs trained in 32 minutes on a 16 GB RTX 4080 Super. One gotcha: Qwen's vocabulary is 152k tokens, so logits for a full 4k sequence don't fit in 16 GB. Compute the loss on target positions only, in checkpointed chunks. Generation: temperature 0.7, top-p 0.9, repetition penalty 1.05.

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

One of the 36 out-of-domain briefs from the [generalization test](#does-it-carry-outside-finance), and what four writers made of it. The first three are Claude Sonnet 5 with the same ghostwriter prompt; the only difference is the learned state, which came entirely from finance columns. The fourth is the [7B LoRA rewriter](#moving-it-into-weights). The brief and everything in it, including the lawsuit, is invented. Each output's headline is shown in bold; the nested quotes are the block quotes it chose to pull from the sources.

The brief:

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

Untrained (empty state):

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

Edit learner (80 edits; guide plus two retrieved finance exemplars):

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

Critique learner (80 sets of notes; guide only):

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

Qwen2.5-7B + LoRA (264 pairs), rewriting the untrained draft above. This is the first sample, not a pick of several, and it's doubly out of domain: the adapter was trained to rewrite Opus drafts of finance sections, and here it gets a Sonnet draft about crab legs.

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

The voice is closer to the target: a flat headline, "What happened? … Well," and shorter, plainer sentences. It also shows why the judge prefers the larger model's prose. Checked against the draft it was given, the rewrite invents a tweet by the lawyer, adds a footnote [3] with nothing pointing to it and a made-up case citation, attributes the restaurant's kitchen-log estimate of the crab to the judge, and opens as if the suit were about the $7,500 emotional-distress claim, which is the part that was thrown out. The in-context learners made none of those mistakes; they had the brief and a much stronger model.

# Appendix: prompts

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
