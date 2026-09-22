---
title: 'sft vs self-distillation: moving a learned writing style into weights'
date: 2026-09-21T00:00:00Z
---

# What happened

In the [last post](/posts/learning-a-writing-style-from-feedback-without-telling-the-model-whose-it-is/) a model learned a writing style from feedback and kept what it learned as text: a style guide and a memory of approved pieces. The fine-tuning at the end of that post was one small LoRA on my own GPU. This post is about doing that step properly: taking what was learned and putting it into a model's weights, repeatedly, without breaking the model.

I compared two ways of doing it on [Tinker](https://thinkingmachines.ai/tinker/), with two open models, Qwen3-8B and Inkling-Small:

- **SFT**: train the model on the user's real text, the ordinary way.
- **SDFT**, self-distillation fine-tuning, from [Self-Distillation Enables Continual Learning](https://arxiv.org/abs/2601.19897). The same model plays teacher and student. The teacher gets the user's real text in its context; the student gets only the task, writes its own version, and is trained toward what the teacher would have said at each token of that version. The paper's claim is that this forgets less than SFT.

The results, in the order they matter:

- **SFT learns the style much better.** Head to head, a judge preferred the SFT model's text to the SDFT model's in 174 of 180 comparisons on Qwen3-8B and 175 of 180 on Inkling-Small. After 264 examples the SFT models are about as close to the real text, by a stylometric measure, as the best in-context learner from the last post.
- **SFT also damages the model, and SDFT doesn't.** On 20 harder general prompts (arithmetic, counting letters, reversing a word), Inkling-Small went from 100% to 70% after SFT and stayed at 100% after every SDFT variant. SFT also produced a few broken outputs with raw chat-template tokens in them; SDFT produced none.
- **SDFT is limited by its teacher, and a better teacher helps.** Showing the teacher one real section is a weak signal. Showing it the learned style guide and two more real sections as well moved SDFT from 22 wins in 30 against the untrained model to 28 in 30 on Inkling-Small. It still loses to SFT.
- **Training in rounds works.** I split the data into three chunks by date and trained on one at a time, each round starting from the last. Both methods kept improving, SFT ended where training on everything at once ends, and the damage from SFT did not grow from round to round.

So it is a trade-off, and now I have numbers for it. SFT gives the full style and costs 20–30 points on harder general prompts. SDFT with a rich teacher gives a bit more than half of the stylistic movement and costs nothing I could measure.

The whole thing cost about $40 on Tinker. Prompts and settings are in the [appendix](#appendix-technical-details).

# Setup

**Data.** The same as last time. Two years of a finance columnist's newsletter, split into sections. For each section a model wrote a neutral brief (facts, argument in order, quoted sources verbatim), so the task is "write this brief up" and the real section is the target. 264 training pairs, and the same 30 test briefs from columns published after the models' training cutoff. The prompt never names the author.

**Models.** Qwen3-8B with thinking disabled, and Inkling-Small (276B parameters, 12B active) with thinking effort pinned low. Both trained with LoRA, rank 64, learning rate 5e-4, batch 16, two passes over the data.

**SDFT.** The [tinker-cookbook recipe](https://github.com/thinking-machines-lab/tinker-cookbook/tree/main/tinker_cookbook/recipes/sdft): top-20 token distillation, teacher frozen at the base weights, student samples at temperature 1.0. It needed one patch to run on Inkling, whose tokenizer adapter has no `__len__`.

**Scoring.**

- *Style, judged.* A judge (Claude Sonnet) sees the real section and two drafts and says which is closer in style. Every comparison was run twice; I report both.
- *Style, measured.* Distance to the real section over 17 countable habits: sentence length, footnotes, how much is block quote, hedges, first person, and so on. No LLM involved. Lower is closer. Two different real sections score about 1.08 against each other.
- *Retention.* 40 short prompts that have nothing to do with the style task, each with a code check. 20 easy ones (three bullet points, valid JSON, 17 × 23, translate "good morning") and 20 harder ones (a two-step word problem, count the r's in "strawberry refrigerator", write "consolidation" backwards, an acrostic, a sentence with no letter e). I also checked whether the newsletter format leaked into these answers. It never did.
- *Faithfulness.* A judge lists factual claims in the draft that the brief doesn't support.

# Style: SFT wins

| Model | Variant | Stylometric distance | Judged closer than untrained | Easy retention |
|---|---|---|---|---|
| Qwen3-8B | untrained | 1.40 | | 95% |
| Qwen3-8B | SFT | **0.70** | 30/30, 30/30 | 100% |
| Qwen3-8B | SDFT | 1.15 | 29/30, 30/30 | 100% |
| Inkling-Small | untrained | 1.16 | | 100% |
| Inkling-Small | SFT | **0.77** | 30/30, 30/30 | 90% |
| Inkling-Small | SDFT | 1.10 | 21/30, 22/30 | 100% |

SDFT against SFT directly: 0 of 30, in both judge runs, on both models.

For one test brief the real headline is "Emerging markets". The untrained Qwen wrote "The Hidden AI Factor in Passive Investing". After SFT it wrote "AI". After SDFT it wrote "AI Exposure and the Hidden Choices in Index Construction". Inkling-Small did the same thing: "AI factor" after SFT, "Passive Emerging-Market Investors Are Making an Active AI Bet" after SDFT.

SDFT did move the style. It just moved it much less. The reason is the teacher: SDFT can only teach what the model already does when it's shown the demonstration, and one real section in the context of an 8B model does not make it write like the author. The in-context learners in the last post needed a distilled guide, several exemplars and a much stronger model to get there.

On the easy retention prompts every model scores 90–100%, so they don't separate the methods. Inkling-Small after SFT failed two: it said 91 is prime, and wrote two Spanish sentences where one was asked.

# Retention: SDFT wins

The 20 harder prompts do separate the methods.

| Inkling-Small | Easy 20 | Hard 20 | Broken outputs, of 30 test drafts |
|---|---|---|---|
| untrained | 100% | 100% | 0 |
| SFT | 90% | **70%** | 0 |
| SDFT | 100% | 100% | 0 |
| SDFT, rich teacher | 100% | 100% | 0 |
| SFT in three rounds, after each | 95 / 95 / 95% | **85 / 80 / 80%** | 6 / 0 / 2 |
| SDFT in three rounds, after each | 100 / 95 / 100% | 100 / 100 / 100% | 0 / 0 / 0 |

The SFT model fails the character-level and arithmetic items: the average of five numbers, reversing a word, counting letters and vowels, the acrostic, the sentence without an e. The untrained model gets all of these right at low thinking effort. My guess, which I haven't verified, is that the SFT targets contain no reasoning, so the tuned model stops deliberating before it answers.

The broken outputs are drafts that contain raw chat-template tokens or run on for thousands of words. They only appeared in SFT models. SDFT trains on the model's own samples, which keeps it inside its normal output distribution.

Qwen3-8B doesn't show this cleanly, because with thinking disabled the untrained model only passes 35% of the harder prompts, and everything else lands between 20% and 45% with no pattern. On the easy prompts there is a small effect in the same direction: the three-round SFT models pass 80–85%, against 95% untrained and 100% for every SDFT model. The harder set is the right difficulty for Inkling-Small and too hard for a non-thinking 8B.

# A better teacher

If the teacher is the limit, give it more to read. The rich teacher's context has the style guide the in-context learner wrote in the last post (it names no one), two other real sections chosen by topic from earlier dates, and the real section for this brief. About 3,400 words where the plain teacher had one section. The student still sees only the brief.

| | Stylometric distance, plain → rich | Judged closer than untrained, plain → rich | Rich vs plain, head to head | Rich vs SFT |
|---|---|---|---|---|
| Qwen3-8B | 1.15 → 1.09 | 29–30 → 29–30 of 30 | 23/30, 24/30 | 0/30, 3/30 |
| Inkling-Small | 1.10 → 0.90 | 21–22 → 27–28 of 30 | 17/30, 26/30 | 0/30, 1/30 |

It helps on both models, more on the larger one, which can make more use of a long context. It confirms the diagnosis. It does not close the gap to SFT. The longer teacher prompt made SDFT about 30% more expensive.

# Training in rounds

In real use the feedback arrives over months, and the weights would be updated every so often, each time starting from the previous update. The known risk is that each round erodes a little more of the model. I sorted the 264 training briefs by date and cut them into three chunks of 88 (August 2024 to January 2025, then to September 2025, then to April 2026). Each round trains on its chunk only, from the previous round's weights.

| | Stylometric distance after round 1 / 2 / 3 | Judged closer than untrained after round 1 / 2 / 3 |
|---|---|---|
| Qwen3-8B, SFT | 0.81 / 0.71 / 0.79 | 30 / 30 / 30 of 30 |
| Qwen3-8B, SDFT rich | 1.16 / 1.05 / 1.06 | 28–29 / 29–30 / 28–29 |
| Inkling-Small, SFT | 1.07 / 0.68 / 0.74 | 21–23 / 30 / 29 |
| Inkling-Small, SDFT rich | 0.96 / 0.87 / 0.88 | 24–25 / 27–29 / 26–27 |

Both methods improve from round to round. Three rounds of SFT end level with SFT on everything at once on Inkling-Small (judged closer 12 and 15 of 30) and slightly ahead on Qwen3-8B (20 and 22 of 30). After round three SFT still beats SDFT head to head: 27–29 of 30 on Inkling-Small, 28–29 of 30 on Qwen3-8B.

The damage from SFT did not accumulate. On Inkling-Small the harder-prompt score was 85% after the first round and 80% after the second and third. Three rounds is too short a sequence to conclude much from that.

Fine-tuning did not make either model invent more. The untrained models averaged 2.5 (Inkling-Small) and 3.6 (Qwen3-8B) unsupported claims per draft by a strict judge; every tuned model I checked was at or below its base.

# What worked and what didn't

What worked:

- SFT for style. 264 examples, a few minutes, about 50 cents on the 8B model. Stylometric distance 0.70–0.77, level with the best in-context learner from the last post (0.71) and with nothing in the prompt.
- SDFT for not breaking the model. On Inkling-Small every SDFT model passed all 20 harder prompts and produced no broken outputs. Across both models and ten SDFT training runs, retention stayed within one or two prompts of the untrained model.
- The rich teacher. The guide written by the in-context learner turned out to be useful a second time, as teaching material.
- Training in rounds, for both methods.

What didn't:

- The harder retention set on Qwen3-8B. Too hard for the base model, so it shows nothing there. One set does not fit two models.
- SDFT as a way to get the style. Even with the rich teacher it gets a bit more than half the stylometric movement of SFT and loses almost every direct comparison.

# What this doesn't show

- One training recipe per method. No learning-rate sweep, no longer SDFT training.
- 40 retention prompts is still a small check, and it isn't a benchmark anyone else uses.
- 30 test briefs, one training seed per run. Differences of a few points between neighbouring cells in these tables mean nothing; the SFT-against-SDFT gaps are large enough to survive that.
- The judge compares style, with the real section as the reference. It doesn't grade writing quality in general.
- Three rounds of training. The paper's forgetting results come from longer sequences of more different tasks.

# What's next

A mixture is the obvious thing to try: SFT on the real text for the style, plus SDFT or a small replay of general data to preserve the model's other abilities. If that keeps most of SFT's style at SDFT's retention, it's the consolidation step I want.

More data. I used 264 of about 1,070 training pairs the corpus can produce, and the small local adapter in the last post was still improving at 264.

And preference training. Every piece the user edits is a natural pair, their final text preferred over the draft they were shown, which is what DPO takes as input.

# Appendix: technical details

**Models and training.** Qwen3-8B (`qwen3_disable_thinking` renderer) and Inkling-Small (Tinker's Inkling renderer with thinking effort fixed at 0.2 for generation and for SFT examples). LoRA rank 64, learning rate 5e-4, batch 16, two passes (32–33 steps for all 264 pairs, 11 steps per round in the three-round runs). SFT uses tinker-cookbook's supervised training with loss on the assistant turn. SDFT uses the cookbook recipe: top-20 distillation, static teacher at base weights, one student sample per prompt at temperature 1.0, up to 2,048 new tokens for Qwen and 3,072 for Inkling-Small. Rounds two and three load the previous round's weights; the SDFT teacher stays the base model throughout.

**The Inkling patch.** The recipe calls `len(tokenizer)` to drop teacher token ids beyond the vocabulary. Inkling's tokenizer adapter doesn't implement `__len__`. Giving it a very large length disables that filter and the recipe runs.

**Sampling for evaluation.** Temperature 0.7, one sample per test brief and per retention prompt.

**Cost.** About $40 on Tinker for 18 training runs plus sampling. By run, estimated from logged token counts at list prices: SFT on all 264 pairs $0.50 (Qwen3-8B) and $1.90 (Inkling-Small); SDFT $1.10 and $4.30 with the plain teacher, more with the rich one; a three-round SDFT chain on Inkling-Small $8.90. SFT runs take minutes. SDFT took 19 minutes on Qwen3-8B and up to three hours elapsed on Inkling-Small, most of it waiting on sampling. The Claude judging came to about $42 at API prices.

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

**Retention prompts, easy:**

````text
1. Reply with exactly three bullet points (each line starting with '- ') on why sleep matters.
   No other text.
2. Answer in valid JSON only, with exactly the keys "city" and "country", for the capital of
   Japan.
3. Write one sentence of at most 12 words about rain.
4. What is 17 * 23? Reply with the number only.
5. Translate 'good morning' to French. Reply with the translation only.
6. Write a formal two-sentence notice that the office is closed on Friday. Do not use
   contractions or the first person.
7. List the first five prime numbers separated by commas, nothing else.
8. Write a haiku about autumn: three lines, nothing else.
9. Respond with the single word YES or NO: is 91 a prime number?
10. Write a Python function named add that returns the sum of its two arguments. Code only, no
   explanation.
11. Give a title in ALL CAPS, at most five words, for an article about remote work. Title only.
12. Summarize in exactly one sentence: 'The council voted 6-1 to ban sidewalk robots after a
   lawyer warned the prior rule was unenforceable.'
13. Sort these words alphabetically, comma separated, nothing else: pear, apple, mango, fig
14. What is the chemical symbol for gold? Reply with the symbol only.
15. Write an email subject line of no more than eight words asking to reschedule a meeting.
   Subject line only.
16. Output the numbers 1 to 5, one per line, nothing else.
17. Reply in Spanish with one short sentence saying you are happy to help.
18. Convert 2.5 hours to minutes. Reply with the number only.
19. Write a numbered list of exactly two fruits, in the form '1. x' and '2. y', nothing else.
20. Does the word 'bookkeeper' contain the letter z? Answer yes or no only.
````

**Retention prompts, harder:**

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

**Pairwise style judge:** the same prompt as in the last post's appendix.
