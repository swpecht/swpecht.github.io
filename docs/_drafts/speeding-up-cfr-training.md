---
layout: post
title:  "Speeding up cfr training"
categories: project-log
---

[*] Get better estimate of the number of istates there should be, iso doesn't seem to be improving anything -- investigate how getting to number
    * Verify suits of face up cards: `todo`
    * Number of hands:
    * Bid phases + discard: `jq .[].infostate[12:] infostates.iso-7m.json | sort | uniq`
[*] Set up features for turning on the suit isomprphism --need to re-train to be able to read the data
[ ] See notes, should switch from num derive and similar to working with enums
[ ] Verify the normalization works as expected -- does it reduce the number of hands? Create some manual tests
    [ ] fix the bug in istate normalization -- should only see 2.4m istates
    [ ] For some reason, see way too many istates for the deal -- looks like some things aren't being sorted properly
        NEED TO RE_SORT THE ISTATE AFTER TRANSLATION
[ ] Change out openhand solver to use feature flags instead of passing config


# Approach
* spades is always the face up card
* linear cfr
* multithreading?
* ...

# Istate estimates

Not seeing any suit call istates, only seeing:

""
"P"
"PP"
"PPP"
"PPPP"
"PPPPP"
"PPPPPP"
"PPPPPPP"
"PPPT|Dis|"
"PPT|Dis|"
"PT|Dis|"
"T|Dis|"

12 items

Deal -- 24 choose 5 * 19 choose 1 = 807576 

9.7m (9690912) infostates with no normalization

$$\binom{24}{5}$$

Normalized

23 choose 5 * 6 choose 1 = 201894

2.4m (2422728) total with normalization

# Results

Run each configuration 3 times -- compare the performance