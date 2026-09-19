---
title: learning a writing style from feedback, without telling the model whose it is
date: 2026-09-19T00:00:00Z
---

# What happened

I wanted to know whether a model can learn a person's preferences continuously, just from the feedback they'd give anyway. Writing style seemed like a good first test, and since I don't have a few hundred of my own edited drafts lying around, I faked the user: the "user" secretly writes like Matt Levine, and the model has to figure that out without ever being told his name.

The short version: after 20 rounds of feedback, a style judge shown the real *Money Stuff* section next to the model's version of the same story picked the model's as the real one 47% of the time. 50% would be a coin flip. The same model told outright to "write in the style of Matt Levine's Money Stuff column," and handed 40 real examples to draw on, managed 17%. With no guidance at all it got 0%.

That's Claude Opus 5 as the writer. Everything it learned lives outside the model: a 450-word style guide that it rewrites as feedback comes in, plus a small memory of texts the user approved. The system rejects any guide that contains an author or publication name, so the guide has to describe techniques — "headline is a flat topic label in sentence case," "end abruptly on one flat, deadpan line" — rather than point at a person. That matters because the real use case is a user who isn't famous. There's no name to point at.

Some of it went the way I expected. Two things didn't. Picking between two drafts, the cheapest feedback a person can give, taught the model nothing at all. And the experiment I designed to show that an unmanaged memory falls apart over time showed the opposite.

I did this with Claude Code driving: it wrote the harness, ran the experiments over about a day and a half, and caught most of the bugs described below by reading its own outputs. All of the prompts are in the [appendix](#appendix-prompts).

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

That last one exists because the same judge model steers the gate and grades the results, which is an obvious way to fool yourself. It earned its keep; more below.

# Which feedback teaches

Claude Sonnet 5 as the writer, 80 feedback events each:

| Feedback | Beats untrained | Beats named-author | Fools lineup | Stylometric distance |
|---|---|---|---|---|
| None (untrained) | — | 17% | 0% | 1.15 |
| Told the name + 40 exemplars | 83% | — | 0% | 1.02 |
| Edits | **100%** | **90%** | 3% | **0.82** |
| Critiques | **100%** | 80% | 0% | 0.88 |
| Picks | 57% | 7% | 0% | 1.37 |

Edits and critiques both blow past the model that was told the answer. Critiques getting to 80% is the result I care about most for a product: a few sentences of notes cost the user almost nothing compared with rewriting a draft, and that learner has no exemplars at all — just 450 words of rules.

Picks taught nothing. 80 of them, and the learner is statistically the untrained model with a worse stylometric score. The two drafts come from the same guide, so they differ too little for the choice to carry information, and the reflection step ends up inventing lessons from noise. Pairwise preference is the standard signal for training reward models, so I assumed it would at least do *something* here. It probably needs deliberately contrasting candidates. As built, it was a control group I didn't plan on having.

# How fast

I reran the first ten events with a guide update after every single one. Share of test briefs where the blind learner beats the named-author model:

| Feedback events | 0 | 1 | 2 | 3 | 5 | 10 |
|---|---|---|---|---|---|---|
| Edits | 13% | 37% | **100%** | 97% | 93% | 93% |
| Critiques | 13% | 0% | 43% | 33% | 53% | 60% |

Two rewrites were enough. Notes are slower and noisier, but get past the named-author model within ten.

One example of what two edits does. For the same test brief, the untrained model's headline was "The AI Question Every Investor Has to Answer, Whether They Like It or Not." After learning, it was "Korea." The real one is "Emerging markets."

# Opus

Same loop, Opus 5 as the writer, 20 events:

| Writer | Beats untrained | Beats named-author | Fools lineup | Stylometric distance |
|---|---|---|---|---|
| Untrained Opus | — | 10% | 0% | 0.93 |
| Told the name + 40 exemplars | 90% | — | 17% | 0.81 |
| Blind learner, 20 critiques | 93% | 63% | 37% | 0.82 |
| Blind learner, 20 edits | **100%** | **83%** | **47%** | **0.71** |

With Sonnet, nothing ever fooled the lineup judge more than 10% of the time, the named-author model included. With Opus the named-author model gets to 17% and the blind learner to 47%.

Two caveats on that table. Only one of four proposed guide updates was accepted in each Opus run, because Opus kept writing 520-word guides against a 450-word budget and my code threw them away instead of compressing them (fixed since). So the edit learner's climb from 30% at five events to 47% at twenty came from the bank growing, not the guide improving. And the critique learner's 37% is one draw from something between 20% and 37% — its guide didn't change between checkpoints and its score did. With 30 test briefs, every percentage in this post carries about ±9 points of noise.

# Deep dive: the run that unlearned

The per-event rerun above is the second attempt. The first one failed in an instructive way.

With updates after every event, the critique learner climbed to about 75% against the untrained model by event three — then fell back to 43% at event ten. The gate had accepted that final guide with 8 wins out of 8 on validation. On the test set it was no better than having learned nothing.

Two things were going on:

1. **The reflector rewrote the whole guide around one critique at a time.** A single set of notes on a single section produced rules like "footnotes are rare" and "build the piece as bullets." The learner's drafts ended up with about five bullet lines per section against 1.7 in the real text.
2. **The gate only compared each candidate with the guide before it.** The judge's pairwise preferences aren't transitive, so a chain of guides can each beat its predecessor while the absolute quality drifts back to where it started. I re-judged the event-ten drafts head to head against the event-five drafts to check: roughly a tie, 13 of 30, even though the event-five guide had beaten untrained 73% of the time and the event-ten guide couldn't.

The fixes are both old ideas from continual learning. **Anchor the gate:** a candidate must beat the current guide on at least 6 of 8 validation briefs *and* score at least as well as the current guide against a fixed set of untrained drafts. **Replay:** show the reflector four past episodes alongside the new one. With both, the curve in the previous section rises and stays up.

The stylometric measure had flagged this run as worse than untrained at *every* checkpoint, including the early ones where the LLM judge was still giving it 70–77%. If I'd only had the judge I would have believed the first three checkpoints.

An earlier version of the gate was worse still: accept on 3 wins out of 5. A guide that's no better than the current one passes that about half the time. The pick learner, which learned nothing, had 5 of 16 updates accepted.

# 200 events: I predicted this wrong

The stability experiment was supposed to be the payoff for all that machinery. Two critique learners, 200 events each. One is gated and budgeted as above. The other appends every lesson it thinks of to an ever-growing list, no gate, no consolidation. I expected the second one to bloat and then degrade.

| After 200 events | Guide size | Beats untrained | Beats named-author | Stylometric distance | Cost per draft |
|---|---|---|---|---|---|
| Gated, budgeted | 489 words | 97% | 83% | 0.85 | $0.020 |
| Append everything | **18,664 words** | 100% | 93% | 0.81 | $0.053, rising |

It bloated. It did not degrade. A long-context model is perfectly happy to write under an 18,000-word, highly repetitive list of lessons. What it costs is money: 2.6 times as much per draft, growing linearly forever.

Meanwhile the gated learner stayed small and cheap and mostly stopped learning. It accepted 3 of 40 proposed rewrites, at events 5, 15 and 175. The rejected candidates won a median of 2–3 of 8 head-to-heads, so the gate wasn't being too strict — those rewrites really were worse than what they'd replace. Rewriting a good 450-word guide from scratch loses information.

So the design I built is the wrong one. The obvious fix is to let lessons accumulate freely and put the gate on the periodic *compression* step instead: learn like the append-everything learner, cost like the gated one.

# Does it carry outside finance?

I froze the learned states and had them write 36 briefs that have nothing to do with finance: a roast chicken headnote, an HR memo about return-to-office tiers, a column about a city council that banned, un-banned and re-banned delivery robots. A judge sees real samples by the author and two drafts and says which sounds more like the same person.

| Head to head | Wins |
|---|---|
| Edits vs picks | 31 of 36 |
| Critiques vs picks | 23 of 36 |
| Edits vs critiques | 29 of 36 |

The pick learner is the control, since it learned nothing in-domain. I had to use it because "beats the untrained model" turned out to be too easy a test out of domain — even the control won that 28 of 36. Any guide that pushes toward a conversational column voice sounds more like a columnist than a default HR memo does. The stylometric measure orders the three learners the same way as the head-to-heads (0.91, 0.99, 1.40, with untrained at 1.24).

# Moving it into weights

The loop saves a (draft, feedback, final text) triple for every event, which is training data. As a first pass I generated untrained Opus drafts for all 264 training briefs and trained a QLoRA adapter on Qwen2.5-7B-Instruct to rewrite an untrained Opus draft into the text the user wanted — a small style layer that sits after the big model. Overnight, on one RTX 4080 Super. The adapter never sees the author's name either.

| Writer | Training pairs | Beats untrained Opus | Fools lineup | Stylometric distance |
|---|---|---|---|---|
| Qwen 7B rewriter, no training | 0 | 0% | 0% | 1.10 |
| Qwen 7B rewriter + LoRA | 20 | 40% | 0% | 0.96 |
| Qwen 7B rewriter + LoRA | 80 | 33% | 7% | 0.95 |
| Qwen 7B rewriter + LoRA | 264 | 33% | 20% | 0.82 |
| Qwen 7B writing from the brief + LoRA | 264 | 67% | 13% | 1.11 |
| *Opus, told the name + 40 exemplars* | — | 90% | 17% | 0.81 |
| *Opus in-context learner, 20 edits* | — | 100% | 47% | 0.71 |

Mixed. More data steadily helps on the lineup test and the stylometric measure, and at 264 pairs a 7B model is roughly level with Opus-told-the-name on both. It also copies structural habits *better* than the in-context learner:

| Habit | Real text | Untrained Opus | Opus in-context learner | Qwen 7B + LoRA |
|---|---|---|---|---|
| Headline length (words) | 2.5 | 5.2 | 2.3 | 2.0 |
| Share of text that is block quote | 0.37 | 0.24 | 0.34 | 0.37 |
| Bullet lines per 1,000 words | 5.4 | 0.0 | 0.0 | 4.4 |
| Footnotes per 1,000 words | 1.0 | 2.9 | 3.1 | 0.7 |

The Opus in-context learner never wrote a single bullet list in 30 sections, even though the real text uses them constantly. The 7B adapter picked that up from examples without anyone writing it down as a rule.

But the pairwise judge still preferred the raw untrained Opus draft two times in three. I read the outputs: the voice is right and the prose is looser and more repetitive than Opus's, with run-on sentences. The adapter learned the style. A 7B model just doesn't write as well as Opus, and the judge notices.

# What worked and what didn't

What worked:

- Blind learning from edits or critiques, fast. Two rewrites or about ten sets of notes to pass a model that was told the answer.
- Banning names from the guide. The rules it wrote are specific and operational, and they transfer to recipes and HR memos.
- Holding content fixed with extracted briefs. Hundreds of tasks for about 4¢ each, and style is the only variable.
- A metric with no LLM in it. It's coarse, but it caught a failure the LLM judge missed.
- Reading outputs end to end. That's how the cleanup step's worst bug turned up: it missed a five-paragraph Bloomberg News quotation introduced only by "Sure sure sure:", which would have left wire-service prose in the "user's" text.

What didn't:

- Pairwise picks between similar drafts. Nothing, after 80.
- My gate, three times: too lenient (3 of 5), then not anchored, then pointed at the wrong step entirely.
- My prediction about unmanaged memory. It gets expensive, not worse.
- The 450-word budget as a hard reject. It cost the Opus runs most of their updates.
- Haiku for cleanup: slow with thinking on, sloppy with it off. Sonnet with thinking on was both more accurate and no more expensive, because it stops rambling in its output.

# What this doesn't show

- The user is simulated, and its "edits" are the real published text. A real person's edits would be partial, inconsistent and noisier.
- The judges are Claude models, and the same family steers the gate. The stylometric measure agrees on direction everywhere but is coarse.
- 30 test briefs. About ±9 points on everything.
- One author. A distinctive one, which probably makes this easier than learning my preferences would be.
- The lineup judge is a model. I built a six-pair "which one is real?" quiz for humans and haven't run it on anyone yet.

# What's next

The hybrid learner: append lessons freely, gate the compression. That falls straight out of the 200-event result and is the first thing I'll build.

Then the human quiz, a judge from a different model family, and picks with deliberately contrasting candidates to see if that signal can be rescued.

On the weights side, a bigger base model is the obvious move, and the loop already saves a draft-versus-final pair for every event, which is what DPO wants as input. The adapter-for-structure, Opus-for-prose split is also worth a try given how well the 7B model copied formatting habits.

And eventually the real version: my own drafts, my own edits, no hidden answer key.

# Appendix: technical details

**Models.** Writer: Claude Sonnet 5 for the feedback-type, speed, stability and generalization experiments; Claude Opus 5 for the headline run. Judge, critic, brief extraction and cleanup: Sonnet 5. Reflection uses the writer model with extended thinking on. Everything ran through headless Claude Code (`claude -p`, tools off, custom system prompt, empty working directory so no project context could leak the author's name) on my subscription. 15,489 calls — 14,748 Sonnet, 736 Opus — which would have been about $437 at API prices.

**Pipeline.**

1. *Parse.* 465 newsletter emails → 374 columns → 1,717 sections with footnotes attached. "Things happen" and podcast sections dropped. 1,325 sections fall in the 300–1,500-word range used as tasks.
2. *Clean.* The export had flattened block quotes into ordinary paragraphs and fused words around links ("now calledExpana"). A model only *points at* problems — which paragraph ids are block quotes, which tokens are fused — and code applies and validates the fixes. No model ever paraphrases the gold text.
3. *Brief.* A model extracts topic, background, points in order, and side points (footnote content goes here without saying it was a footnote), in note form. Quoted sources are attached verbatim. Briefs mentioning the author or publication are rejected by a regex lint.
4. *Split by date.* Train through April 2026, validate May–June, test July–August.

**Learner settings.** Guide budget 450 words (candidates over 495 are compressed, then rejected if still over). Two exemplars retrieved per draft by TF-IDF cosine on the brief's topic and background. Default batch of 5 events per update; the speed experiment uses 1. Replay: 4 past episodes per reflection. Gate: 8 validation briefs on rotation, accept if the candidate wins ≥ 6 head-to-heads and its win count against fixed untrained drafts is at least the current guide's. Judge position is randomized on every call.

**Stylometric features** (17): headline length; sentence-length mean and standard deviation; share of sentences ≤ 6 words; words per paragraph; block-quote share of all words; and per-1,000-word rates of bullet lines, footnotes, question marks, parentheses, dashes, colons, inline quotations, first person, second person, hedges ("sort of," "I guess," "you know," …) and contractions. Each feature's gap is divided by that feature's standard deviation across real sections, then averaged.

**LoRA.** Qwen2.5-7B-Instruct, 4-bit NF4 with double quantization, bf16 compute. LoRA r=16, α=32, dropout 0.05 on all attention and MLP projections (40M trainable parameters). 3 epochs, lr 1e-4 cosine with 5% warmup, batch 1 × 8 accumulation, max 4,096 tokens, loss on the target text only. 264 pairs trained in 32 minutes on a 16 GB RTX 4080 Super. One gotcha: Qwen's vocabulary is 152k tokens, so logits for a full 4k sequence don't fit in 16 GB. Compute the loss on target positions only, in checkpointed chunks. Generation: temperature 0.7, top-p 0.9, repetition penalty 1.05.

**An example critique.** What the simulated user sends back in the critique condition:

> The voice is a bit too polished/columnist-y for this — I want more of that plain, almost thinking-out-loud quality, like "Will he win? I don't know" instead of dressing it up with lines like "nobody does, which is the entire point of the exercise." Cut the little flourishes (the "not a comfortable sentence to type" aside, the footnote joke about the CEO tweet) — they're fun but they're not really how I talk, and they make it feel more written-at-you than thought-through-with-you. Let the block quotes breathe more; pull in the extra Axios and DealBook lines (the Eissenstat quote, the Feldman quote) instead of trimming them down, since the sourcing is half the piece. Keep the math footnote but make it plainer — just state the two conditional probabilities cleanly rather than narrating them. And drop "the good part" framing — I'd rather just move to the next fact than editorialize about how interesting it is.

**A learned guide.** The Opus edit learner's guide in full. The short quoted fragments are its own micro-examples:

````text
- **Headline is a flat topic label in sentence case:** e.g. "Meal allowance", "Bitcoin custody", or just a ticker. No clever titles, puns, colons or title case.
- **Open with the facts, not a riff:** name the source and state the basic facts, or pose the plain question, in the first sentence ("Here's a weird little trade, first reported by X of Y."). Never open with a staged scene or an invented problem ("Here is a thing about dollars…").
- **Stage the argument plainly:** give the background, then the normal expectation ("Ideally you would (1)… (2)… (3)…"), then the weird thing that actually happened. Ask the reader's question and answer it casually: "Why would the investors do this? Well…" Label approaches loosely ("what you might call legal realism").
- **Sound like talking, not polished prose:** use hedges and admitted uncertainty ("I have no idea?", "I suppose", "seems sort of plausible"). Use ellipses for comic pauses ("It … didn't work, is the short summary"), plus "um," "like," and taking back a joke ("I am just kidding. I mean, I'm not…"). Put made-up quotes in ordinary people's mouths ("I am a simple widget importer…").
- **Cut literary polish:** no aphorisms, no neat groups of three, no clever inverted phrases ("off-grid money, laboriously re-gridded"), no "Which brings us to…", no "I want to be clear." Use small parenthetical asides instead, e.g. "(Coinbase?)".
- **Quote sources at length:** use several block-quote paragraphs, keeping context the reader could have done without. Introduce them plainly ("Bloomberg's X reports:", "Anyway here's a Bloomberg story about…", "From the prospectus:"). React afterward in a short, unguarded line: "Yeah the article is about…", "Imagine working at headquarters and thinking this!"
- **End abruptly on one flat, deadpan line:** often a standalone fact or an obvious-sounding verdict ("Yes the highest form of crypto wealth is obviously turning it into a stock."). No wrap-up paragraph, no moral, no "it is also quite funny."
- **Footnotes hold the numbers and asides:** conditional-probability arithmetic, checks against the data ("Just for fun, I looked at…"), disclaimers ("Not legal advice!"), side quotes and minor caveats. Keep them short, sometimes one line. Keep the main text readable as a story.
- **No subheadings, bold or emphasis italics in the body:** use a bulleted list for a sequence of timestamped events. Put multi-part reasons inline as "(1)… (2)…".
- **Names:** use the full formal company name on first mention (Meta Platforms Inc., BlackRock Inc.) and the full title and name for officials ("President Donald Trump"). Mention earlier columns casually ("we have discussed around here").
- **Paragraphs:** medium length, each moving one step. Use plain connectors ("Now,", "But", "Still", "Anyway") rather than rhetorical transitions.
````

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

Final versions the client approved for earlier briefs. They show the voice to match; do not reuse their content:

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
- Concrete and operational. "Be conversational" is useless; say what to do and what to avoid, with
  a tiny invented illustration when it helps. Prefer rules that would have fixed several episodes
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
introduced and reacted to, paragraphing, formatting habits such as headings, lists and footnotes.
Ignore which facts each covers, factual accuracy, and length unless wildly different. You must
pick one.
````

Lineup judge:

````text
You are an authorship attribution expert. You get REFERENCE samples by one author,
then two texts, A and B, on the same subject. Exactly one of A and B was written by the author of
the references; the other is an imitation. Decide which is the real one, judging by style (voice,
humor, rhythm, structure, habits), not by subject matter. You must pick one.
````

Out-of-domain judge:

````text
You are an authorship attribution expert. You get REFERENCE samples by one author, who
writes about finance. Then two texts, A and B, on some unrelated subject. Decide which of A and B
sounds more like it was written by the author of the references, judging by transferable style
(voice, humor, rhythm, how arguments are staged, how sources are handled, formatting habits), not
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
   paragraphs, and may be introduced by nothing more than a short remark. A paragraph in the columnist's own voice that merely
   contains a short inline quotation in quotation marks is NOT a block quote, and
   neither is a paragraph that mixes a quotation with the columnist's own sentences:
   when in doubt, it is not a block quote.

2. spacing_fixes: tokens where a space went missing and two words are fused, e.g.
   {"wrong": "calledExpana", "right": "called Expana"}, {"wrong": "bysuing", "right": "by suing"}.
   `wrong` must be copied exactly from the text and contain no spaces; `right` must be
   the same characters with only space(s) inserted. Do not report anything else
   (no typo fixes, no hyphenation opinions, no legitimate compounds or brand names).
````

LoRA rewriter system prompt:

````text
You are the client's editor. Rewrite the ghostwriter's DRAFT of a newsletter section the way the client would have written it themselves. Keep the substance and the quoted sources. Markdown: a `## ` heading, `> ` block quotes, footnotes as [n] markers listed at the end. Output only the rewritten section.
````
