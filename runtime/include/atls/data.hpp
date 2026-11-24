#pragma once

#include "./expr.hpp"

#include <algotradelib/strategy>

#include <chrono>
#include <cstddef>
#include <deque>
#include <optional>
#include <utility>

/**
 * @file
 * @brief What a program with `data {}` reads: the record `on_data` takes and the tape that gives each bar its value.
 */

namespace atls {

/**
 * @brief The record a program with `data {}` takes through `on_data`: one value of one name, for one
 *   instrument or for every one.
 */
struct DataRecord {
  /** @brief The value's index in the program's `data {}` (the order of `atls_data()`). */
  std::size_t name_{0};
  /** @brief The instrument it is for; empty for every instrument of the program. */
  std::optional<atl::InstrumentId> instrument_;
  /** @brief The value; NaN is an undefined value. */
  double value_{ops::NaN};
};

/**
 * @brief One value of `data {}`: the received values not yet reached by a bar, and the value at the last bar.
 *
 * `at(end)` gives the latest value known by `end` (received at or before it), NaN before the first; values
 * received later stay for later bars, so a bar delivered after them (a backlog, warm-up history) never reads a
 * value from after its end. Bars arrive in time order, so the tape holds only what no bar has reached yet.
 */
class DataTape {
public:
  /**
   * @brief Keeps a received value.
   * @param at When it became known.
   * @param value The value.
   */
  void add(std::chrono::system_clock::time_point at, double value) {
    pending_.emplace_back(at, value);
  }

  /**
   * @brief Returns the latest value known by `end`, dropping what it supersedes.
   * @param end The bar's period end.
   * @return The value, or NaN before the first value known by `end`.
   */
  double at(std::chrono::system_clock::time_point end) {
    while (!pending_.empty() && !(end < pending_.front().first)) {
      current_ = pending_.front().second;
      pending_.pop_front();
    }
    return current_;
  }

private:
  std::deque<std::pair<std::chrono::system_clock::time_point, double>> pending_;
  double current_{ops::NaN};
};

} // namespace atls
