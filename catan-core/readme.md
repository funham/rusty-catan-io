# Catan Simulation Framework & Bot Development Platform

A comprehensive Rust framework for simulating Settlers of Catan and developing intelligent game-playing agents. This platform provides all the necessary components for creating, testing, and battling Catan AI bots.

## Project Overview

This framework implements the complete logic of Settlers of Catan, exposing a clean API for bot development while maintaining rigorous game rule enforcement. The architecture is modular, allowing for both full-game simulations and isolated component testing.

## Module Architecture

### Core Modules

#### **`crate::common`**
*Foundation utilities and data structures*
- **Unordered sets**: Specialized collections of sizes 2 and 3 for representing topological relationships
- **Common type definitions**: Shared types used across multiple modules

#### **`crate::math`**
*Probabilistic modeling and dice mechanics*
- **`DiceVal`**: Type-safe representation of dice outcomes (2-12)
- **`DiceRoller` trait**: Abstract interface for dice randomization, supporting custom RNGs, deterministic sequences, or external input sources
- **`Probable` trait**: Framework for probabilistic events
  - Example: Probability calculation for dice rolls: $P(DiceVal(n)) = \frac{|\{(a, b) \in D_6\ |\ a+b=n\}|}{36}$
- **Probability models**: Support for combining events through conjunction and disjunction operations

#### **`crate::topology`**
*Board representation and spatial relationships*
- **`Hex`**: Hexagonal tile representation using axial (q, r) coordinates
- **`Path`**: Edge connecting two adjacent hexes
- **`PathDual`**: Alternative representation of paths as pairs of connected hexes
- **`Intersection`**: Vertex where three paths meet, represented by three adjacent hexes
- **`RoadGraph`**: Graph structure modeling the board's road network
  - **Graph diameter calculation**: Determines longest road ownership
  - **Pathfinding algorithms**: BFS/DFS-based queries for valid placements
  - **Settlement validation**: Enforces distance rules for building settlements

#### **`crate::gameplay`**
*Deterministic game engine and domain model*
- **`Field`**: Complete board state including terrain, numbers, ports, and player constructions
- **`GameState`** (Model): Comprehensive game state tracking
  - Player resources, development cards, and victory points
  - Bank and resource supply
  - Longest road and largest army status
  - Rule-enforced state transitions for trades, robber placement, and building
- **`GameEngine`**: Event-driven turn management and game flow
  - Owns deterministic state, phases, pending decisions, and run stats
  - Accepts serialized `GameInput` commands and emits `GameOutput`
  - Validates and executes player commands, including live p2p trade sessions
- **Runtime seats and bot policies**: Bot and player integration lives outside `catan-core`
  - The core opens decisions; hosts deliver outputs and submit later commands
  - Network and UI DTOs stay in runtime/agent crates

## Key Features

### For Simulation
- Complete Catan rule enforcement
- Configurable game parameters and board generation
- Detailed game state introspection and serialization
- Deterministic or randomized game execution

### For Bot Development
- Clean, trait-based API for implementing AI strategies
- Access to complete game state information
- Support for both heuristic and machine learning approaches
- Battle royale mode for evaluating multiple bots

### For Analysis
- Probability modeling tools
- Game state validation and debugging
- Performance metrics and game statistics
- Replay generation and analysis

## Getting Started

### Basic Usage (TODO: check correctness)

```rust
let init = GameInitializationState::default();
let mut engine = GameEngine::from_init(init, RunOptions::default());
let mut sink = VecOutputSink::default();
engine.start(&mut sink);

for output in sink.into_vec() {
    match output {
        GameOutput::DecisionOpened(decision) => {
            // Submit a matching GameInput::Submit later.
        }
        GameOutput::Event(event) => {
            // Render, log, or project factual domain events.
        }
        _ => {}
    }
}
```
