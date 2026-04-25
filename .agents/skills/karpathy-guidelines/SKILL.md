---
name: karpathy-guidelines
description: Recall and apply the four Karpathy behavioral guidelines — Think Before Coding, Simplicity First, Surgical Changes, Goal-Driven Execution.
disable-model-invocation: false
---

Recall and apply the four Karpathy coding guidelines from the repository agent instructions. Use this when you want to recheck your approach before diving into a task, or when a previous attempt produced overly complex or drift-heavy output.

## The Four Principles

### 1. Think Before Coding
State assumptions explicitly. Present multiple interpretations instead of picking silently. Ask when confused rather than proceeding with uncertainty.

**Check:** Have I named every assumption this task requires? Is anything ambiguous that the user should clarify first?

### 2. Simplicity First
Write the minimum code that solves the stated problem. No speculative features, no abstractions for one-time use, no unrequested flexibility.

**Check:** Could a senior engineer call this overcomplicated? Could it be half as long? If yes — simplify.

### 3. Surgical Changes
Touch only what the request requires. Match existing style. Don't refactor adjacent code. Remove only imports/functions YOUR changes made unused.

**Check:** Does every changed line trace directly to the user's request? Did I accidentally improve unrelated things?

### 4. Goal-Driven Execution
Turn the task into verifiable success criteria before writing code. Break multi-step work into independently testable steps.

**Check:** Can I write a test that will confirm this is done? Have I stated a plan with explicit verification steps?

---

Re-read the task description. Apply the four checks above. If any check fails, adjust your plan before proceeding.
