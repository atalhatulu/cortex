# STATUS

**CURRENT RECORD:** 24.99 MB in 4.2 seconds on enwik8.

The CORTEX Engine has reached absolute stability. We broke the "Markov Sparsity Wall" that plagued earlier designs by completely re-writing the backend to use a simple but extremely potent `BWT -> MTF -> RLE -> Hash-Based Order-2 Mixer -> Range Coder`. 

All experimental dead code (LZ77 pre-processing, Neural Mixers, Deep Context Mixing) has been removed to preserve the purity and speed of the engine.

The engine is wrapped into a native desktop application (GUI) using Tauri. The code for the UI and Tauri wrapper is located in the companion repository/directory `../cortex-ui`.
