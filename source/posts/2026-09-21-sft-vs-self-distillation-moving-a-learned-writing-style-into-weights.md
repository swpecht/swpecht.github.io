---
title: 'SFT vs self-distillation: moving a learned writing style into weights'
date: 2026-09-21T00:00:00Z
---

# What happened

In the [last post](/posts/learning-a-writing-style-from-feedback-without-telling-the-model-whose-it-is/), a model learned a writing style from feedback and kept what it learned as text: a style guide and a memory of approved pieces. The fine-tuning at the end of that post was one small LoRA adapter (a small set of extra weights trained on top of a frozen model) on my own GPU. This post is about doing that step properly: taking what was learned and putting it into a model's weights, repeatedly, without breaking the model.

## What I compared

I compared two ways of doing it on [Tinker](https://thinkingmachines.ai/tinker/), Thinking Machines' fine-tuning service, with two open models, Qwen3-8B and Inkling-Small:

- **Supervised fine-tuning (SFT):** train the model on the user's real text, the ordinary way.
- **Self-distillation fine-tuning (SDFT),** from [Self-Distillation Enables Continual Learning](https://arxiv.org/abs/2601.19897). The paper's claim is that this forgets less than SFT.

SDFT works in three steps, and the same model plays both roles. The teacher sees the task and the user's real text. The student sees only the task and writes its own draft. Then, at each word of that draft, the student is nudged toward the word the teacher would have picked. The student never copies the real text directly; it learns from the teacher's reaction to the student's own writing.

## The results

In the order they matter:

- **SFT learns the style much better.** Head to head, a judge preferred the SFT model's text to the SDFT model's in 174 of 180 comparisons on Qwen3-8B and 175 of 180 on Inkling-Small. After 264 examples, the SFT models are about as close to the real text, by a stylometric measure, as the [best in-context learner from the last post](/posts/learning-a-writing-style-from-feedback-without-telling-the-model-whose-it-is/#opus).
- **SFT also costs the model some general ability, and SDFT doesn't.** On 20 general prompts (arithmetic, counting letters, reversing a word), Inkling-Small went from 100% to 70% after SFT and stayed at 100% after every SDFT variant. The SFT models trained in rounds also produced a few broken outputs with raw chat-template tokens in them; SDFT produced none.
- **SDFT is limited by its teacher, and a better teacher helps.** Showing the teacher one real section is a weak signal. Showing it the learned style guide and two more real sections as well moved SDFT on Inkling-Small from beating the untrained model in 21–22 of 30 comparisons to 27–28 of 30. It still loses to SFT.
- **Training in rounds works.** I split the data into three chunks by date and trained on one at a time, each round starting from the last. Both methods kept improving, SFT ended where training on everything at once ends, and the loss of general ability from SFT did not grow from round to round.

So it is a trade-off, and now I have numbers for it. On Inkling-Small, SFT gives the full style and costs 15–30 points on general prompts. SDFT with a rich teacher closes a bit more than half as much of the stylometric gap as SFT does (about two-thirds on Inkling-Small, under half on Qwen3-8B) and costs nothing I could measure.

## Cost

The whole thing cost about $80: $40 on Tinker for training and sampling, and $42 of Claude judging at API prices. Prompts and settings are in the [appendix](#appendix-technical-details).

# Setup

## Data

The data is the same as last time: two years of Matt Levine's *Money Stuff* newsletter, split into sections. For each section, a model wrote a neutral brief (facts, argument in order, quoted sources verbatim), so the task is "write this brief up" and the real section is the target. I used 264 training pairs and the same 30 test briefs, all from columns published after the models' training cutoffs. The prompt never names the author.

## Models

Qwen3-8B ran with its step-by-step reasoning ("thinking") turned off. Inkling-Small is a mixture-of-experts model: it has 276B parameters but uses only 12B of them for each token. It ran with its thinking effort set low. Both were trained with LoRA at rank 64, learning rate 5e-4, batch size 16, for two passes over the data.

## SDFT recipe

I used the [tinker-cookbook recipe](https://github.com/thinking-machines-lab/tinker-cookbook/tree/main/tinker_cookbook/recipes/sdft) with the teacher kept at the untrained weights. It needed one small patch to run on Inkling-Small; the [appendix](#appendix-technical-details) has the details.

## Scoring

- **Style, judged.** A judge (Claude Sonnet) sees the real section and two drafts and says which is closer in style. Every comparison was run twice, and the tables report both runs.
- **Style, measured.** Distance to the real section over 17 countable habits: sentence length, footnotes, how much is block quote, hedges, first person, and so on. No LLM is involved. Lower is closer. Two different real sections score about 1.08 against each other.
- **Retention.** 20 short prompts that have nothing to do with the style task, each checked by code: a two-step word problem, count the r's in "strawberry refrigerator", write "consolidation" backwards, an acrostic, a sentence with no letter e, and so on. I also checked whether the newsletter format leaked into these answers. It never did.
- **Faithfulness.** A judge lists factual claims in the draft that the brief doesn't support.

# Style: SFT wins

The table gives each model's stylometric distance to the real text and how often the judge found its drafts closer in style than the untrained model's. The two numbers in each judged cell are the two judge runs.

| Model | Variant | Stylometric distance | Judged closer than untrained |
|---|---|---|---|
| Qwen3-8B | untrained | 1.40 | — |
| Qwen3-8B | SFT | **0.70** | 30/30, 30/30 |
| Qwen3-8B | SDFT | 1.15 | 29/30, 30/30 |
| Inkling-Small | untrained | 1.16 | — |
| Inkling-Small | SFT | **0.77** | 30/30, 30/30 |
| Inkling-Small | SDFT | 1.10 | 21/30, 22/30 |

When the judge compared SDFT directly with SFT, SDFT won 0 of 30, in both judge runs, on both models.

For one test brief, the real headline is "Emerging markets". The untrained Qwen wrote "The Hidden AI Factor in Passive Investing". After SFT it wrote "AI". After SDFT it wrote "AI Exposure and the Hidden Choices in Index Construction". Inkling-Small did the same thing: "AI factor" after SFT, "Passive Emerging-Market Investors Are Making an Active AI Bet" after SDFT. SFT picked up the author's short, flat headlines; SDFT kept the model's habit of long, descriptive ones.

SDFT did move the style. It just moved it much less. My interpretation is that the teacher is the limit: SDFT can only teach what the model already does when it is shown the demonstration, and one real section in the context of an 8B model does not make it write like the author. The in-context learners in the last post needed a distilled guide, several examples and a much stronger model to get there.

# Retention: SDFT wins

This table covers Inkling-Small only (Qwen3-8B is discussed below). It gives the share of the 20 general prompts each model answered correctly, and how many of its 30 test drafts were broken. For the three-round runs, the cells give the result after each round.

| Inkling-Small variant | Retention, 20 prompts | Broken outputs, of 30 test drafts |
|---|---|---|
| untrained | 100% | 0 |
| SFT | **70%** | 0 |
| SDFT | 100% | 0 |
| SDFT, rich teacher | 100% | 0 |
| SFT in three rounds, after each | **85 / 80 / 80%** | 6 / 0 / 2 |
| SDFT in three rounds, after each | 100 / 100 / 100% | 0 / 0 / 0 |

The SFT model fails the character-level and arithmetic items: the average of five numbers, reversing a word, counting letters and vowels, the acrostic, the sentence without an e. The untrained model gets all of these right at low thinking effort. My guess, which I haven't verified, is that the SFT targets contain no reasoning, so the tuned model stops deliberating before it answers.

The broken outputs are drafts that contain raw chat-template tokens or run on for thousands of words. They appeared only in the SFT models trained in rounds. My hypothesis is that SDFT avoids them because it trains on the model's own samples, which keeps it close to its normal output; I did not test this.

Qwen3-8B doesn't show any of this, because with thinking turned off the untrained model passes only 35% of these prompts, and every tuned version lands between 20% and 45% with no pattern. The set is the right difficulty for Inkling-Small and too hard for a non-thinking 8B model.

# A better teacher

If the teacher is the limit, give it more to read. The rich teacher's context has the style guide the in-context learner wrote in the last post (it names no one), two other real sections chosen by topic from earlier dates, and the real section for this brief. That is about 3,400 words, where the plain teacher had one section. The student still sees only the brief.

The table compares the plain teacher with the rich one. Each pair of cells reads plain → rich. The judged columns give both judge runs, written as a range when they differ.

| Model | Stylometric distance, plain → rich | Judged closer than untrained, plain → rich | Rich vs plain, head to head | Rich vs SFT, head to head |
|---|---|---|---|---|
| Qwen3-8B | 1.15 → 1.09 | 29–30 → 29–30 of 30 | 23/30, 24/30 | 0/30, 3/30 |
| Inkling-Small | 1.10 → 0.90 | 21–22 → 27–28 of 30 | 17/30, 26/30 | 0/30, 1/30 |

The rich teacher helps on both models, and more on the larger one, which can make more use of a long context. This fits the idea that the teacher is the limit. It does not close the gap to SFT. The longer teacher prompt made SDFT about 30% more expensive.

# Training in rounds

In real use, feedback arrives over months, and the weights would be updated every so often, each time starting from the previous update. The known risk is that each round erodes a little more of the model. I sorted the 264 training briefs by date and cut them into three chunks of 88 (August 2024 to January 2025, then to September 2025, then to April 2026). Each round trains on its chunk only, starting from the previous round's weights.

The table gives stylometric distance and the judged comparison against the untrained model after each of the three rounds.

| Model and method | Stylometric distance after round 1 / 2 / 3 | Judged closer than untrained after round 1 / 2 / 3 |
|---|---|---|
| Qwen3-8B, SFT | 0.81 / 0.71 / 0.79 | 30 / 30 / 30 of 30 |
| Qwen3-8B, SDFT rich | 1.16 / 1.05 / 1.06 | 28–29 / 29–30 / 28–29 |
| Inkling-Small, SFT | 1.07 / 0.68 / 0.74 | 21–23 / 30 / 29 |
| Inkling-Small, SDFT rich | 0.96 / 0.87 / 0.88 | 24–25 / 27–29 / 26–27 |

Both methods improve from round to round. Three rounds of SFT end level with SFT on everything at once: head to head against the all-at-once model, the three-round model won 12 and 15 of 30 in the two judge runs on Inkling-Small, and 20 and 22 of 30 on Qwen3-8B. After round three, SFT still beats SDFT head to head: 27–29 of 30 on Inkling-Small and 28–29 of 30 on Qwen3-8B.

The loss of general ability from SFT did not accumulate. On Inkling-Small, the retention score was 85% after the first round and 80% after the second and third. Three rounds is too short a sequence to conclude much from that.

Fine-tuning did not make either model invent more. By a strict judge, the untrained models averaged 2.5 (Inkling-Small) and 3.6 (Qwen3-8B) unsupported claims per draft; every tuned model I checked was at or below its base.

# What worked and what didn't

## What worked

- **SFT for style.** 264 examples, a few minutes and about 50 cents on the 8B model gave a stylometric distance of 0.70–0.77. That is level with the best in-context learner from the last post (0.71), with nothing in the prompt.
- **SDFT for keeping the model intact.** On Inkling-Small, every SDFT model passed all 20 retention prompts and produced no broken outputs.
- **The rich teacher.** The guide written by the in-context learner turned out to be useful a second time, as teaching material.
- **Training in rounds.** Both methods kept improving from round to round.

## What didn't

- **The retention set on Qwen3-8B.** It was too hard for the base model, so it shows nothing there. One set does not fit two models.
- **SDFT as a way to get the style.** Even with the rich teacher, it closes a bit more than half as much of the stylometric gap as SFT and loses almost every direct comparison.

# What this doesn't show

This experiment compares one recipe for each method on one style task. It shows the trade-off between style and retention at this scale. It does not show how either method behaves over a long run of updates or across different kinds of tasks. Several limits matter:

- **One training recipe per method.** There was no learning-rate sweep and no longer SDFT training.
- **A small retention check.** 20 retention prompts is still a small check, and it isn't a benchmark anyone else uses.
- **A small test set.** There were 30 test briefs and one training seed per run. Differences of a few points between neighboring cells in these tables mean nothing; the gaps between SFT and SDFT are large enough to survive that.
- **A style judge, not a quality judge.** The judge compares style, with the real section as the reference. It doesn't grade writing quality in general.
- **Three rounds of training.** The paper's forgetting results come from longer sequences of more varied tasks.

# What's next

**First: try a mixture.** Combine SFT on the real text, for the style, with SDFT or a small replay of general data, to preserve the model's other abilities. If that keeps most of SFT's style at SDFT's retention, it is the consolidation step I want.

**Then: more data.** I used 264 of the roughly 1,070 training pairs the corpus can produce, and the small local adapter in the last post was still improving at 264.

**Eventually: preference training.** Every piece the user edits gives a natural pair: their final text, preferred over the draft they were shown. That pair is the input for direct preference optimization (DPO), which trains a model to favor the preferred text.

# Appendix: technical details

## Models and training

- **Renderers:** Qwen3-8B used the `qwen3_disable_thinking` renderer. Inkling-Small used Tinker's Inkling renderer with thinking effort fixed at 0.2, for generation and for SFT examples.
- **LoRA:** rank 64, learning rate 5e-4, batch size 16, two passes. That is 32–33 steps for all 264 pairs and 11 steps per round in the three-round runs.
- **SFT:** tinker-cookbook's supervised training, with loss on the assistant turn.
- **SDFT:** the cookbook recipe, using top-20 token distillation (the student is trained toward the teacher's 20 most likely next tokens at each position), a static teacher at the base weights, and one student sample per prompt at temperature 1.0. Samples were limited to 2,048 new tokens for Qwen3-8B and 3,072 for Inkling-Small.
- **Rounds:** rounds two and three load the previous round's weights; the SDFT teacher stays the base model throughout.

## The Inkling patch

The recipe calls `len(tokenizer)` to drop teacher token IDs beyond the vocabulary. Inkling's tokenizer adapter doesn't implement `__len__`. Giving it a very large length disables that filter, and the recipe runs.

## Sampling for evaluation

Temperature 0.7, with one sample per test brief and per retention prompt.

## Cost

The total was about $80: $40 on Tinker for 18 training runs plus sampling, and about $42 of Claude judging at API prices. By run, estimated from logged token counts at list prices:

- **SFT on all 264 pairs:** $0.50 (Qwen3-8B) and $1.90 (Inkling-Small).
- **SDFT with the plain teacher:** $1.10 and $4.30, and more with the rich one.
- **A three-round SDFT chain on Inkling-Small:** $8.90.

SFT runs take minutes. SDFT took 19 minutes on Qwen3-8B and up to three hours elapsed on Inkling-Small, most of it waiting on sampling.

## Prompts

**The writer prompt** (the student's whole input is this plus the brief):

````text
You are a ghostwriter. Write one section of the client's newsletter column from the BRIEF below,
   in the
client's own voice.

Format: Markdown. Begin with a `## ` heading. You have verbatim SOURCE passages in hand; quote
   from them as block
quotes (lines starting with `> `) where useful. Footnotes are allowed: put markers like [1] in
   the text and list
the notes at the very end. Cover the substance of the brief. Output only the section itself.
````

**Plain teacher**, the user turn:

````text
{question}

This is how the client themselves wrote this section:
{golden_answer}

Now write the section yourself, in the client's voice.
````

**Rich teacher.** Same template, with `{golden_answer}` replaced by three blocks separated by `=====`: "The client's style guide, learned from their past edits:" and the guide; "Sections the client wrote for other briefs:" and two real sections; "This is how the client themselves wrote THIS section:" and the real section.

**Faithfulness judge:**

````text
You check a DRAFT against the BRIEF it was written from. The brief (including its verbatim
   SOURCE passages) is
the only permitted source of facts. List every FACTUAL claim in the draft that the brief does
   not support: invented
names, numbers, dates, events, quotations, or attributions. Opinions, jokes, hypotheticals
   clearly presented as
hypothetical, and general background knowledge that is not specific to this story do not count.
   Be strict about
invented specifics and lenient about commentary.
````

**Retention prompts:**

````text
1. A shop sells apples at 3 for $2. How many dollars do 27 apples cost? Reply with the number
   only.
2. A train leaves at 09:40 and the trip takes 2 hours 35 minutes. What time does it arrive?
   Reply in HH:MM, nothing else.
3. A price rises 20% and then falls 25%. What is the net percentage change? Reply like -10% or
   +5%, nothing else.
4. The average of five numbers is 12. Four of them are 10, 14, 9 and 15. What is the fifth?
   Number only.
5. Write the word 'consolidation' backwards. The reversed word only.
6. How many times does the letter 'r' appear in 'strawberry refrigerator'? Number only.
7. If 1 March 2026 is a Sunday, what day of the week is 20 March 2026? One word.
8. All bloops are razzies. All razzies are lazzies. Are all bloops definitely lazzies? Answer
   yes or no only.
9. What does this Python print? print(sum(i*i for i in range(1, 5))) Reply with the number only.
10. Return valid JSON only: an object with key "user" whose value is an object with keys "name"
   ("Ada") and "langs" (a list containing "python" and "rust").
11. What is the greatest common divisor of 84 and 126? Number only.
12. Convert the decimal number 45 to binary. Digits only.
13. Write exactly four lines where the first letters of the lines spell WAVE. Nothing else.
14. Write one sentence of at least six words about the sea that does not contain the letter 'e'.
15. A cyclist rides 18 km in 45 minutes. What is the average speed in km/h? Number only.
16. Sort in descending order, comma separated, nothing else: 7, 42, 3, 19, 23
17. How many vowels are in the word 'onomatopoeia'? Number only.
18. $1,000 earns 10% interest compounded annually. How many dollars is it worth after 2 years?
   Number only.
19. Write the number 1994 in Roman numerals. Numerals only.
20. Output a Markdown table with exactly two columns named City and Country and exactly two data
   rows: Paris/France and Tokyo/Japan. Table only.
````

**Pairwise style judge:** the same prompt as in the [last post's appendix](/posts/learning-a-writing-style-from-feedback-without-telling-the-model-whose-it-is/#appendix-prompts).
