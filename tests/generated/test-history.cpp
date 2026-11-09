// The history a live runner requests (`warmup_bars()`) against the gate, for a partial sum over a lag under a skip window (tests/generated/programs/partial-history-*.atls).
// The history covers every window whole and follows `set`, while the first target still comes at the gate, over the partial sum.
#include "replay.hpp"

#include <partial_history_named.hpp>
#include <partial_history_single.hpp>
#include <partial_history_universe.hpp>

#include <catch2/catch_test_macros.hpp>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;

/**
 * @brief Checks the history with the defaults and after each parameter is set: k + h + m - 2 bars, while the gate is max(k + 1, m).
 * @param make Makes the strategy with the defaults.
 */
template <class Make>
void check_history(const Make& make) {
  CHECK(make().warmup_bars() == 16);
  CHECK(t::with(make(), {{"k", 7}}).warmup_bars() == 18);
  CHECK(t::with(make(), {{"h", 20}}).warmup_bars() == 26);
  CHECK(t::with(make(), {{"m", 10}}).warmup_bars() == 23);
  // Setting back gives the defaults' figure again.
  CHECK(t::with(make(), {{"k", 7}, {"k", 5}}).warmup_bars() == 16);
  // One sign in the sum and a window of one: nothing decides before it is whole, so the figure is the gate's.
  CHECK(t::with(make(), {{"h", 2}, {"m", 1}}).warmup_bars() == 6);
}

/**
 * @brief The fills of `symbol`.
 * @param outcome A replay.
 * @param symbol The symbol.
 * @return How many.
 */
std::size_t fills_of(const t::Outcome& outcome, const std::string& symbol) {
  std::size_t n = 0;
  for (const auto& e : outcome.result_.executions_) {
    n += e.fill_.instrument_.symbol_ == symbol;
  }
  return n;
}

/**
 * @brief The bars up to `last`, included.
 * @param all The series.
 * @param last A bar's time.
 * @return The series cut there.
 */
std::vector<t::Series> until(std::vector<t::Series> all, std::chrono::system_clock::time_point last) {
  for (auto& series : all) {
    series.bars_.erase(
      std::remove_if(
        series.bars_.begin(), series.bars_.end(),
        [last](const atl::Bar& bar) {
          return bar.timestamp_ > last;
        }
      ),
      series.bars_.end()
    );
  }
  return all;
}

} // namespace

TEST_CASE("partial history: one instrument") {
  const auto id = t::id_of("A");
  check_history([&id] {
    return atls::PartialHistorySingle(id);
  });
  // Decided on the gate's bar (the 6th) over one sign: a replay fills a decision at the next bar, so 6 bars fill nothing and 7 fill it.
  const std::vector<t::Series> all{t::walk("A", 11, 120)};
  const auto over = [&](std::size_t bars) {
    const auto cut = until(all, all[0].bars_[bars - 1].timestamp_);
    return fills_of(t::run(atls::PartialHistorySingle(id), {"A"}, t::daily_events(cut)), "A");
  };
  CHECK(over(6) == 0);
  CHECK(over(7) == 1);
}

TEST_CASE("partial history: named instruments") {
  const std::vector<atl::InstrumentId> ids{t::id_of("Y"), t::id_of("X")};
  check_history([&ids] {
    return atls::PartialHistoryNamed(ids);
  });
  const std::vector<t::Series> all{t::walk("Y", 12, 120), t::walk("X", 13, 120)};
  const auto over = [&](std::size_t bars) {
    const auto cut = until(all, all[0].bars_[bars - 1].timestamp_);
    return fills_of(t::run(atls::PartialHistoryNamed(ids), {"Y", "X"}, t::daily_events(cut)), "Y");
  };
  CHECK(over(6) == 0);
  CHECK(over(7) == 1);
}

TEST_CASE("partial history: a universe, each instrument at its own gate") {
  const std::vector<atl::InstrumentId> ids{t::id_of("A"), t::id_of("B")};
  check_history([&ids] {
    return atls::PartialHistoryUniverse(ids);
  });
  // B is listed from day 30: its gate is its own 6th bar.
  const std::vector<t::Series> all{t::walk("A", 14, 120), t::walk("B", 15, 120, 0, 30)};
  const auto over = [&](std::size_t k, std::size_t bars) {
    const auto cut = until(all, all[k].bars_[bars - 1].timestamp_);
    return fills_of(t::run(atls::PartialHistoryUniverse(ids), {"A", "B"}, t::daily_events(cut)), all[k].symbol_);
  };
  CHECK(over(0, 6) == 0);
  CHECK(over(0, 7) == 1);
  CHECK(over(1, 6) == 0);
  CHECK(over(1, 7) == 1);
}
