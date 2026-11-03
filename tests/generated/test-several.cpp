// Programs over several instruments (tests/programs/): named instruments, and a universe with session handlers
// whose instruments miss bars under `missing = skip`, each generated in every variant.
#include "replay.hpp"

#include <pairs_exact.hpp>
#include <pairs_incremental.hpp>
#include <pairs_keep.hpp>
#include <pairs_naive.hpp>
#include <pairs_noshare.hpp>
#include <universe_exact.hpp>
#include <universe_incremental.hpp>
#include <universe_keep.hpp>
#include <universe_naive.hpp>
#include <universe_noshare.hpp>

#include <catch2/catch_test_macros.hpp>

#include <chrono>
#include <map>
#include <set>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;

ATL_ASSERT_STRATEGY(atls::PairsIncremental, atl::strategy::MinimalContext);
ATL_ASSERT_STRATEGY(atls::UniverseIncremental, atl::strategy::MinimalContext);

/** Every variant's fills and report equal the first's; returns the first. */
t::Outcome same_fills(const std::vector<t::Outcome>& runs) {
  for (std::size_t k = 1; k < runs.size(); ++k) {
    INFO("variant " << k);
    CHECK(t::signature(runs[k]) == t::signature(runs[0]));
  }
  CHECK(runs.front().report_.rejected_ == 0);
  CHECK(runs.front().report_.refused_ == 0);
  return runs.front();
}

} // namespace

TEST_CASE("pairs: two named instruments, long and short, the same fills in every variant") {
  const std::vector<t::Series> all{t::walk("Y", 4, 1500), t::walk("X", 5, 1500)};
  const auto events = t::daily_events(all);
  const std::vector<std::string> symbols{"Y", "X"};
  const std::vector<atl::InstrumentId> ids{t::id_of("Y"), t::id_of("X")};
  t::Settings shorts;
  shorts.shorts_ = true;
  const auto first = same_fills({
    t::run(atls::PairsIncremental(ids), symbols, events, shorts),
    t::run(atls::PairsNaive(ids), symbols, events, shorts),
    t::run(atls::PairsExact(ids), symbols, events, shorts),
    t::run(atls::PairsNoshare(ids), symbols, events, shorts),
    t::run(atls::PairsKeep(ids), symbols, events, shorts),
  });
  std::set<std::string> traded;
  for (const auto& e : first.result_.executions_) {
    traded.insert(e.fill_.instrument_.symbol_);
  }
  CHECK(traded == std::set<std::string>{"X", "Y"});
  CHECK(first.result_.executions_.size() > 10);
  CHECK_THROWS_AS(atls::PairsIncremental({ids[0]}), std::invalid_argument);
}

TEST_CASE("universe: session handlers and missing bars, the same fills in every variant") {
  // C lists after 300 days; every instrument misses about one day in 15.
  const std::vector<t::Series> all{
    t::walk("A", 6, 800, 15), t::walk("B", 7, 800, 15), t::walk("C", 8, 800, 15, 300), t::walk("D", 9, 800, 15)
  };
  const auto events = t::session_events(all);
  const std::vector<std::string> symbols{"A", "B", "C", "D"};
  std::vector<atl::InstrumentId> ids;
  for (const auto& s : symbols) {
    ids.push_back(t::id_of(s));
  }
  t::Settings close;
  close.decision_close_ = true;
  close.slippage_ticks_ = 0;
  const auto first = same_fills({
    t::run(atls::UniverseIncremental(ids), symbols, events, close),
    t::run(atls::UniverseNaive(ids), symbols, events, close),
    t::run(atls::UniverseExact(ids), symbols, events, close),
    t::run(atls::UniverseNoshare(ids), symbols, events, close),
    t::run(atls::UniverseKeep(ids), symbols, events, close),
  });
  // A target stated at the open fills on the day's bar (stamped at the close, 06:30 UTC) at that day's open; one
  // stated at the close fills on the instrument's next open point (00:00 UTC) at the close it was decided on.
  std::map<std::string, std::vector<atl::Bar>> bars;
  for (const auto& s : all) {
    bars[s.symbol_] = s.bars_;
  }
  const auto day_of = [](std::chrono::system_clock::time_point at) {
    return std::chrono::duration_cast<std::chrono::hours>(at.time_since_epoch()).count() / 24;
  };
  std::set<std::string> traded;
  for (const auto& e : first.result_.executions_) {
    const auto minutes = std::chrono::duration_cast<std::chrono::minutes>(e.fill_.at_.time_since_epoch()).count();
    const bool buy = e.fill_.side_ == atl::OrderSide::BUY;
    CHECK(minutes % (24 * 60) == (buy ? 390 : 0));
    const auto& own = bars[e.fill_.instrument_.symbol_];
    const atl::Bar* decided = nullptr;
    for (const auto& b : own) {
      if (buy ? day_of(b.timestamp_) == day_of(e.fill_.at_) : day_of(b.timestamp_) < day_of(e.fill_.at_)) {
        decided = &b;
      }
    }
    REQUIRE(decided != nullptr);
    CHECK(e.fill_.price_ == (buy ? *decided->open_ : decided->close_));
    traded.insert(e.fill_.instrument_.symbol_);
  }
  CHECK(traded == std::set<std::string>{"A", "B", "C", "D"});
  CHECK(first.result_.executions_.size() > 20);
}
