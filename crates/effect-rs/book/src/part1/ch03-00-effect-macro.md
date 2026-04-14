# The effect! Macro — Do-Notation for Mortals

Chapter 2 introduced the `effect!` macro as "syntactic sugar for `flat_map`." That's technically accurate, but undersells it. In practice, `effect!` is how you write almost every multi-step computation in effect-rs.

This chapter covers the why, the how, and the limits of the macro. By the end you'll be fluent in `~`, comfortable handling errors inside the macro, and clear on when *not* to use it.
