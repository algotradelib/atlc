#pragma once

#include "./ops.hpp"

#include <vector>

/**
 * @file
 * @brief One compiled single-instrument strategy run over several instruments: a slot per instrument, fed
 *   bar by bar (`on_bar`) or one period's slice at a time (`on_bars`).
 */

namespace atls {

/**
 * @brief Forwards each bar to the slot trading its instrument.
 * @tparam S A compiled single-instrument strategy.
 */
template <class S>
struct Universe {
  /** @brief The slots keep their own lookback; so does the universe. */
  static constexpr bool reads_bar_history = S::reads_bar_history;

  /**
   * @brief Forwards the bar to its slot; a bar for no slot is ignored.
   * @param ctx The runner's context.
   * @param bar The bar.
   */
  template <class Ctx>
  void on_bar(Ctx& ctx, const atl::strategy::BarEvent& bar) {
    for (auto& slot : slots_) {
      if (slot.instrument_ == bar.instrument_) {
        slot.on_bar(ctx, bar);
        return;
      }
    }
  }

  /** @brief One strategy per instrument. */
  std::vector<S> slots_;
};

/**
 * @brief Takes one period's bars at once and forwards each to its slot.
 * @tparam S A compiled single-instrument strategy.
 */
template <class S>
struct UniverseBars {
  /** @brief The slots keep their own lookback; so does the universe. */
  static constexpr bool reads_bar_history = S::reads_bar_history;

  /**
   * @brief Forwards every bar of the slice to its slot; a bar for no slot is ignored.
   * @param ctx The runner's context.
   * @param slice The period's bars.
   */
  template <class Ctx>
  void on_bars(Ctx& ctx, const atl::strategy::BarSlice& slice) {
    for (const auto& bar : slice.bars_) {
      for (auto& slot : slots_) {
        if (slot.instrument_ == bar.instrument_) {
          slot.on_bar(ctx, bar);
          break;
        }
      }
    }
  }

  /** @brief One strategy per instrument. */
  std::vector<S> slots_;
};

} // namespace atls
