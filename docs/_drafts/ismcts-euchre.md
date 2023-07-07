---
layout: post
title:  "Information state monte carlo tree search (ISMCTS) for Euchre"
categories: project-log
---

[*] Fix error with ISMCTS -- the istate for discard a
  * JsJhKhQdKd|KcTsQsTdJd|JcQc9sAsQh|AcKs9hThAh|Ad|PT|Ad|
  * TsJsThJhKh|Kc9sQhTdJd|QcQsAs9dKd|TcJcKs9hQd|Ah|PT|Qd|
  * The information state for dealer to discard and for a different player to play are identical. So see an issue where player 0 is getting the same hand that the dealer had previously when they needed to discard
  * Need to update the istate key (not string!) to differentiate
[ ] Change to modifying the gamestate rather than cloning it
[ ] Tune ISMCTS algorithm -- can it beat PIMCTS?
  * Tune with fill games


# Intro to ISMCTS

# Performance on other games, expoitability

# Performance on euchre -- head to head


## Scratch


https://dan.bravender.net/2017/6/9/Over_1_billion_tricks_played_-_Information_Set_Monte_Carlo_Tree_Search_Euchre_simulation_database.html


starting tune rune for: ISMCTS, n=1000
liars_poker_bot::scripts::tune: 2023-07-03T22:08:18+08:00 - INFO - fiinal_policy	selection	uct_c	max_simulations	avg score
liars_poker_bot::scripts::tune: 2023-07-03T22:09:03+08:00 - INFO - MaxVisitCount        Uct     0.001   5       -0.512
liars_poker_bot::scripts::tune: 2023-07-03T22:10:09+08:00 - INFO - MaxVisitCount        Uct     0.001   10      -0.279
liars_poker_bot::scripts::tune: 2023-07-03T22:11:34+08:00 - INFO - MaxVisitCount        Uct     0.001   15      -0.18
liars_poker_bot::scripts::tune: 2023-07-03T22:13:12+08:00 - INFO - MaxVisitCount        Uct     0.001   20      -0.065
liars_poker_bot::scripts::tune: 2023-07-03T22:15:51+08:00 - INFO - MaxVisitCount        Uct     0.001   50      -0.029
liars_poker_bot::scripts::tune: 2023-07-03T22:20:22+08:00 - INFO - MaxVisitCount        Uct     0.001   100     0.049
liars_poker_bot::scripts::tune: 2023-07-03T22:53:11+08:00 - INFO - MaxVisitCount        Uct     0.001   1000    0.119
liars_poker_bot::scripts::tune: 2023-07-03T22:53:55+08:00 - INFO - MaxVisitCount        Uct     0.1     5       -0.593
liars_poker_bot::scripts::tune: 2023-07-03T22:54:57+08:00 - INFO - MaxVisitCount        Uct     0.1     10      -0.245
liars_poker_bot::scripts::tune: 2023-07-03T22:56:21+08:00 - INFO - MaxVisitCount        Uct     0.1     15      -0.117
liars_poker_bot::scripts::tune: 2023-07-03T22:57:44+08:00 - INFO - MaxVisitCount        Uct     0.1     20      -0.039
liars_poker_bot::scripts::tune: 2023-07-03T23:00:29+08:00 - INFO - MaxVisitCount        Uct     0.1     50      0.01
liars_poker_bot::scripts::tune: 2023-07-03T23:05:01+08:00 - INFO - MaxVisitCount        Uct     0.1     100     0.016
liars_poker_bot::scripts::tune: 2023-07-03T23:38:38+08:00 - INFO - MaxVisitCount        Uct     0.1     1000    0.032
liars_poker_bot::scripts::tune: 2023-07-03T23:39:32+08:00 - INFO - MaxVisitCount        Uct     0.5     5       -0.585
liars_poker_bot::scripts::tune: 2023-07-03T23:40:41+08:00 - INFO - MaxVisitCount        Uct     0.5     10      -0.202
liars_poker_bot::scripts::tune: 2023-07-03T23:42:04+08:00 - INFO - MaxVisitCount        Uct     0.5     15      -0.258
liars_poker_bot::scripts::tune: 2023-07-03T23:43:40+08:00 - INFO - MaxVisitCount        Uct     0.5     20      -0.098
liars_poker_bot::scripts::tune: 2023-07-03T23:46:32+08:00 - INFO - MaxVisitCount        Uct     0.5     50      0.02
liars_poker_bot::scripts::tune: 2023-07-03T23:51:24+08:00 - INFO - MaxVisitCount        Uct     0.5     100     0.123
liars_poker_bot::scripts::tune: 2023-07-04T00:24:50+08:00 - INFO - MaxVisitCount        Uct     0.5     1000    0.146
liars_poker_bot::scripts::tune: 2023-07-04T00:25:46+08:00 - INFO - MaxVisitCount        Uct     1       5       -0.438
liars_poker_bot::scripts::tune: 2023-07-04T00:26:54+08:00 - INFO - MaxVisitCount        Uct     1       10      -0.24
liars_poker_bot::scripts::tune: 2023-07-04T00:28:15+08:00 - INFO - MaxVisitCount        Uct     1       15      -0.091
liars_poker_bot::scripts::tune: 2023-07-04T00:29:50+08:00 - INFO - MaxVisitCount        Uct     1       20      -0.049
liars_poker_bot::scripts::tune: 2023-07-04T00:32:39+08:00 - INFO - MaxVisitCount        Uct     1       50      0.065
liars_poker_bot::scripts::tune: 2023-07-04T00:37:30+08:00 - INFO - MaxVisitCount        Uct     1       100     0.145
liars_poker_bot::scripts::tune: 2023-07-04T01:10:07+08:00 - INFO - MaxVisitCount        Uct     1       1000    0.267
liars_poker_bot::scripts::tune: 2023-07-04T01:10:54+08:00 - INFO - MaxVisitCount        Uct     3       5       -0.545
liars_poker_bot::scripts::tune: 2023-07-04T01:12:01+08:00 - INFO - MaxVisitCount        Uct     3       10      -0.333
liars_poker_bot::scripts::tune: 2023-07-04T01:13:22+08:00 - INFO - MaxVisitCount        Uct     3       15      -0.121
liars_poker_bot::scripts::tune: 2023-07-04T01:14:55+08:00 - INFO - MaxVisitCount        Uct     3       20      -0.038
liars_poker_bot::scripts::tune: 2023-07-04T01:17:45+08:00 - INFO - MaxVisitCount        Uct     3       50      0.123
liars_poker_bot::scripts::tune: 2023-07-04T01:22:36+08:00 - INFO - MaxVisitCount        Uct     3       100     0.173
liars_poker_bot::scripts::tune: 2023-07-04T01:55:28+08:00 - INFO - MaxVisitCount        Uct     3       1000    0.36
liars_poker_bot::scripts::tune: 2023-07-04T01:56:12+08:00 - INFO - MaxVisitCount        Uct     5       5       -0.594
liars_poker_bot::scripts::tune: 2023-07-04T01:57:17+08:00 - INFO - MaxVisitCount        Uct     5       10      -0.385
liars_poker_bot::scripts::tune: 2023-07-04T01:58:38+08:00 - INFO - MaxVisitCount        Uct     5       15      -0.21
liars_poker_bot::scripts::tune: 2023-07-04T02:00:12+08:00 - INFO - MaxVisitCount        Uct     5       20      0.002
liars_poker_bot::scripts::tune: 2023-07-04T02:03:04+08:00 - INFO - MaxVisitCount        Uct     5       50      0.111
liars_poker_bot::scripts::tune: 2023-07-04T02:08:00+08:00 - INFO - MaxVisitCount        Uct     5       100     0.23
liars_poker_bot::scripts::tune: 2023-07-04T02:41:17+08:00 - INFO - MaxVisitCount        Uct     5       1000    0.378
liars_poker_bot::scripts::tune: 2023-07-04T02:42:01+08:00 - INFO - MaxVisitCount        Puct    0.001   5       -0.468
liars_poker_bot::scripts::tune: 2023-07-04T02:43:09+08:00 - INFO - MaxVisitCount        Puct    0.001   10      -0.151
liars_poker_bot::scripts::tune: 2023-07-04T02:44:33+08:00 - INFO - MaxVisitCount        Puct    0.001   15      -0.153
liars_poker_bot::scripts::tune: 2023-07-04T02:46:08+08:00 - INFO - MaxVisitCount        Puct    0.001   20      -0.006
liars_poker_bot::scripts::tune: 2023-07-04T02:49:03+08:00 - INFO - MaxVisitCount        Puct    0.001   50      0.016
liars_poker_bot::scripts::tune: 2023-07-04T02:53:57+08:00 - INFO - MaxVisitCount        Puct    0.001   100     0.072
liars_poker_bot::scripts::tune: 2023-07-04T03:27:42+08:00 - INFO - MaxVisitCount        Puct    0.001   1000    0.124
liars_poker_bot::scripts::tune: 2023-07-04T03:28:25+08:00 - INFO - MaxVisitCount        Puct    0.1     5       -0.466
liars_poker_bot::scripts::tune: 2023-07-04T03:29:32+08:00 - INFO - MaxVisitCount        Puct    0.1     10      -0.159
liars_poker_bot::scripts::tune: 2023-07-04T03:30:56+08:00 - INFO - MaxVisitCount        Puct    0.1     15      -0.125
liars_poker_bot::scripts::tune: 2023-07-04T03:32:33+08:00 - INFO - MaxVisitCount        Puct    0.1     20      -0.072
liars_poker_bot::scripts::tune: 2023-07-04T03:35:28+08:00 - INFO - MaxVisitCount        Puct    0.1     50      -0.004
liars_poker_bot::scripts::tune: 2023-07-04T03:40:24+08:00 - INFO - MaxVisitCount        Puct    0.1     100     0.094
liars_poker_bot::scripts::tune: 2023-07-04T04:14:11+08:00 - INFO - MaxVisitCount        Puct    0.1     1000    0.122
liars_poker_bot::scripts::tune: 2023-07-04T04:15:07+08:00 - INFO - MaxVisitCount        Puct    0.5     5       -0.423
liars_poker_bot::scripts::tune: 2023-07-04T04:16:17+08:00 - INFO - MaxVisitCount        Puct    0.5     10      -0.118
liars_poker_bot::scripts::tune: 2023-07-04T04:17:39+08:00 - INFO - MaxVisitCount        Puct    0.5     15      -0.084
liars_poker_bot::scripts::tune: 2023-07-04T04:19:16+08:00 - INFO - MaxVisitCount        Puct    0.5     20      -0.01
liars_poker_bot::scripts::tune: 2023-07-04T04:22:11+08:00 - INFO - MaxVisitCount        Puct    0.5     50      0.09
liars_poker_bot::scripts::tune: 2023-07-04T04:27:06+08:00 - INFO - MaxVisitCount        Puct    0.5     100     0.093
liars_poker_bot::scripts::tune: 2023-07-04T05:00:27+08:00 - INFO - MaxVisitCount        Puct    0.5     1000    0.201
liars_poker_bot::scripts::tune: 2023-07-04T05:01:12+08:00 - INFO - MaxVisitCount        Puct    1       5       -0.509
liars_poker_bot::scripts::tune: 2023-07-04T05:02:19+08:00 - INFO - MaxVisitCount        Puct    1       10      -0.194
liars_poker_bot::scripts::tune: 2023-07-04T05:03:42+08:00 - INFO - MaxVisitCount        Puct    1       15      -0.104
liars_poker_bot::scripts::tune: 2023-07-04T05:05:18+08:00 - INFO - MaxVisitCount        Puct    1       20      0.01
liars_poker_bot::scripts::tune: 2023-07-04T05:08:10+08:00 - INFO - MaxVisitCount        Puct    1       50      0.107
liars_poker_bot::scripts::tune: 2023-07-04T05:13:06+08:00 - INFO - MaxVisitCount        Puct    1       100     0.101
liars_poker_bot::scripts::tune: 2023-07-04T05:46:25+08:00 - INFO - MaxVisitCount        Puct    1       1000    0.161
liars_poker_bot::scripts::tune: 2023-07-04T05:47:09+08:00 - INFO - MaxVisitCount        Puct    3       5       -0.464
liars_poker_bot::scripts::tune: 2023-07-04T05:48:18+08:00 - INFO - MaxVisitCount        Puct    3       10      -0.306
liars_poker_bot::scripts::tune: 2023-07-04T05:49:43+08:00 - INFO - MaxVisitCount        Puct    3       15      -0.251
liars_poker_bot::scripts::tune: 2023-07-04T05:51:22+08:00 - INFO - MaxVisitCount        Puct    3       20      -0.257
liars_poker_bot::scripts::tune: 2023-07-04T05:54:17+08:00 - INFO - MaxVisitCount        Puct    3       50      -0.174
liars_poker_bot::scripts::tune: 2023-07-04T05:59:13+08:00 - INFO - MaxVisitCount        Puct    3       100     -0.042
liars_poker_bot::scripts::tune: 2023-07-04T06:32:32+08:00 - INFO - MaxVisitCount        Puct    3       1000    0.118
liars_poker_bot::scripts::tune: 2023-07-04T06:33:30+08:00 - INFO - MaxVisitCount        Puct    5       5       -0.562
liars_poker_bot::scripts::tune: 2023-07-04T06:34:43+08:00 - INFO - MaxVisitCount        Puct    5       10      -0.348
liars_poker_bot::scripts::tune: 2023-07-04T06:36:09+08:00 - INFO - MaxVisitCount        Puct    5       15      -0.427
liars_poker_bot::scripts::tune: 2023-07-04T06:37:47+08:00 - INFO - MaxVisitCount        Puct    5       20      -0.346
liars_poker_bot::scripts::tune: 2023-07-04T06:40:41+08:00 - INFO - MaxVisitCount        Puct    5       50      -0.349
liars_poker_bot::scripts::tune: 2023-07-04T06:45:36+08:00 - INFO - MaxVisitCount        Puct    5       100     -0.223
liars_poker_bot::scripts::tune: 2023-07-04T07:19:05+08:00 - INFO - MaxVisitCount        Puct    5       1000    0.0
liars_poker_bot::scripts::tune: 2023-07-04T07:19:52+08:00 - INFO - NormalizedVisitedCount       Uct     0.001   5       -0.657
liars_poker_bot::scripts::tune: 2023-07-04T07:21:01+08:00 - INFO - NormalizedVisitedCount       Uct     0.001   10      -0.415
liars_poker_bot::scripts::tune: 2023-07-04T07:22:25+08:00 - INFO - NormalizedVisitedCount       Uct     0.001   15      -0.38
liars_poker_bot::scripts::tune: 2023-07-04T07:24:03+08:00 - INFO - NormalizedVisitedCount       Uct     0.001   20      -0.305
liars_poker_bot::scripts::tune: 2023-07-04T07:26:58+08:00 - INFO - NormalizedVisitedCount       Uct     0.001   50      -0.12
liars_poker_bot::scripts::tune: 2023-07-04T07:31:59+08:00 - INFO - NormalizedVisitedCount       Uct     0.001   100     -0.034
liars_poker_bot::scripts::tune: 2023-07-04T08:05:37+08:00 - INFO - NormalizedVisitedCount       Uct     0.001   1000    0.049
liars_poker_bot::scripts::tune: 2023-07-04T08:06:23+08:00 - INFO - NormalizedVisitedCount       Uct     0.1     5       -0.66
liars_poker_bot::scripts::tune: 2023-07-04T08:07:32+08:00 - INFO - NormalizedVisitedCount       Uct     0.1     10      -0.416
liars_poker_bot::scripts::tune: 2023-07-04T08:08:56+08:00 - INFO - NormalizedVisitedCount       Uct     0.1     15      -0.363
liars_poker_bot::scripts::tune: 2023-07-04T08:10:32+08:00 - INFO - NormalizedVisitedCount       Uct     0.1     20      -0.303
liars_poker_bot::scripts::tune: 2023-07-04T08:13:25+08:00 - INFO - NormalizedVisitedCount       Uct     0.1     50      -0.183
liars_poker_bot::scripts::tune: 2023-07-04T08:18:24+08:00 - INFO - NormalizedVisitedCount       Uct     0.1     100     -0.073
liars_poker_bot::scripts::tune: 2023-07-04T08:52:31+08:00 - INFO - NormalizedVisitedCount       Uct     0.1     1000    0.089
liars_poker_bot::scripts::tune: 2023-07-04T08:53:18+08:00 - INFO - NormalizedVisitedCount       Uct     0.5     5       -0.664
liars_poker_bot::scripts::tune: 2023-07-04T08:54:27+08:00 - INFO - NormalizedVisitedCount       Uct     0.5     10      -0.549
liars_poker_bot::scripts::tune: 2023-07-04T08:55:52+08:00 - INFO - NormalizedVisitedCount       Uct     0.5     15      -0.404
liars_poker_bot::scripts::tune: 2023-07-04T08:57:29+08:00 - INFO - NormalizedVisitedCount       Uct     0.5     20      -0.277
liars_poker_bot::scripts::tune: 2023-07-04T09:00:22+08:00 - INFO - NormalizedVisitedCount       Uct     0.5     50      -0.166
liars_poker_bot::scripts::tune: 2023-07-04T09:05:17+08:00 - INFO - NormalizedVisitedCount       Uct     0.5     100     -0.072
