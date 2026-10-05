# Uniform Inventory

A DSP's uniform catalog and its stock counts, with their change journal, in one database. Stock
writes are deltas and catalog edits never set a count, so concurrent adjustments add up. Its
page follows other sessions' changes through a long poll.
