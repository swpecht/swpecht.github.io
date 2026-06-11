---
title: Comparing training time cost for euchre strategies
date: 2026-06-07T00:00:00Z
---

Rework this to compare the training time required for different strategies, using both euchre and oh hell. Structure:

1-3 sentence summary of findings

Short introduction to what each strategy is including links to the papers.

Table of findings for euchre: train time of 0 is pimcts (note for entire table that inference time is excluded), then cfr0, R-NaD over CPU (estimated), go-mcts over CPU (estimated), and then the GPU training times for go-mcts and R-NaD. And then the relative performance vs pimcts.

Table of relative performance of each model vs each other.

Repeat both tables for oh hell.

Appendix:
* Overview of the hardware 
* How cpu times were estimated 
* How win rates were calculated 
