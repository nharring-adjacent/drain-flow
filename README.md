# Drain Flow

Implementations of [Drain](https://jiemingzhu.github.io/pub/pjhe_icws2017.pdf) starting with an extremely naive single level approach and building towards a hopefully highly efficient implementation using [DifferentialDataflow](https://github.com/TimelyDataflow/differential-dataflow) with arbitrary depth.

## What is drain?

Drain is a technique for parsing and grouping log lines using a fixed depth parse tree. Parsing happens in several stages, see the linked paper for full details but the general idea is that first we run a pass of domain specific regexs to eliminate fields with expected variation (think leading timestamps on a log file which convey nothing about the similarity of log content), then selects a parse tree based on the number of words/tokens in the message, and finally descending into a token tree of arbitrary depth.

At the bottom of the tree is a vector of potential log matches which are the same length and begin with the same N tokens (or matching wildcards). Each match is scored by comparing the token in each position and awarding a point for each match.

The top-scoring message is then evaluated against as score / length > threshold and either added as an example of the existing record or added as a new record. When messages are added as new examples an additional pass is done to find any non-matching tokens and replace them with wildcards.
