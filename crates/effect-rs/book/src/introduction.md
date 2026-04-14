# Introduction

Welcome to *A Playful Guide to Typed Effects in Rust*.

You're here because you've written async Rust. You've wrangled `Future`s, battled `Pin`, and accumulated a small mountain of `.await?` chains. Maybe you've built something that works. Maybe it even works well.

But something feels off.

Your functions take twelve parameters. Your error types are an enum with forty variants. Testing requires a mock factory factory. And every time you refactor, the compiler screams at you for three hours before you figure out which lifetime annotation you forgot.

There's a better way.

This book teaches you **effect-rs**, a library for building composable, testable, type-safe async programs in Rust. Instead of functions that *do* things, you'll write functions that *describe* what they want to do. Instead of passing dependencies through every function, you'll declare them in the type signature and let the compiler enforce them. Instead of hoping you remembered to clean up that connection, you'll have guaranteed finalizers that run no matter what.

## Who This Book Is For

You should know Rust basics: ownership, borrowing, traits, async/await. You don't need to know category theory, functional programming, or what a monad is. (We'll get there, but gently.)

If you've ever thought "there has to be a better way to structure this async code," this book is for you.

## How to Read This Book

**Part I: Foundations** explains *why* effects exist and teaches you the basics. Start here.

**Part II: Environment & Dependencies** covers the `R` parameter — the secret weapon for compile-time dependency injection. This is where effect-rs really shines.

**Part III: Real Programs** covers error handling, concurrency, resources, and scheduling — everything you need for production code.

**Part IV: Advanced** dives into STM, streams, schemas, and testing. Read these when you're ready to go deep.

Code examples are runnable. When you see a code block, you can trust it compiles (unless marked otherwise).

Let's begin.
