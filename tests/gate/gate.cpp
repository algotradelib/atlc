// The differential gate: compiled programs, in every arithmetic and with rings shared or not, against the same
// strategies written by hand (hand.hpp) and against the library's SmaCrossover, over generated daily bars.
// Every fill and the report must be identical; the run must compare exactly the expected number of cases.
#include "hand.hpp"
#include "replay.hpp"

#include <pullback_exact.hpp>
#include <pullback_incremental.hpp>
#include <pullback_naive.hpp>
#include <pullback_noshare.hpp>
#include <sma_exact.hpp>
#include <sma_incremental.hpp>
#include <sma_naive.hpp>
#include <sma_noshare.hpp>
#include <trail_exact.hpp>
#include <trail_incremental.hpp>
#include <trail_naive.hpp>
#include <trail_noshare.hpp>

#include <algotradelib/vanilla>

#include <cstdio>
#include <string>
#include <utility>
#include <vector>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;

/** Three strategies, three points each, two bar series. */
constexpr int EXPECTED_CASES = 18;

/** The replay settings: costs as on the Korean exchange, one tick of slippage, long only. */
t::Settings settings() {
  t::Settings s;
  s.cash_ = "100000000";
  return s;
}

/** Compares each route with the first; prints one line; whether all agreed. */
bool compare(const std::string& label, const std::vector<std::pair<std::string, t::Outcome>>& routes, int& comparisons) {
  const auto reference = t::signature(routes.front().second);
  std::string failed;
  for (std::size_t r = 1; r < routes.size(); ++r) {
    ++comparisons;
    if (t::signature(routes[r].second) != reference) {
      failed += " " + routes[r].first;
    }
  }
  std::printf("%s %s (%zu fills)%s%s\n", failed.empty() ? "ok  " : "DIFF", label.c_str(), routes.front().second.result_.executions_.size(),
    failed.empty() ? "" : ":", failed.c_str());
  return failed.empty() && !routes.front().second.result_.executions_.empty();
}

} // namespace

int main() {
  int cases = 0;
  int good = 0;
  int comparisons = 0;
  const auto s = settings();
  for (const unsigned seed : {1u, 2u}) {
    const std::string symbol = "00000" + std::to_string(seed);
    const auto bars = t::walk(symbol, seed, 2500);
    const auto events = t::daily_events({bars});
    const std::vector<std::string> symbols{symbol};
    const auto id = t::id_of(symbol);
    const auto tag = [&](const char* what) {
      return std::string(what) + " seed " + std::to_string(seed);
    };

    for (const auto& [fast, slow] : std::vector<std::pair<std::int64_t, std::int64_t>>{{5, 20}, {20, 50}, {10, 120}}) {
      atl::strategy::vanilla::MovingAverageCrossover::Params q;
      q.instrument_ = id;
      q.fast_ = static_cast<std::size_t>(fast);
      q.slow_ = static_cast<std::size_t>(slow);
      q.quantity_ = atl::Decimal64(100, 0);
      const auto compiled = [&](auto strategy) {
        // `slow` is bounded below by `fast + 1`, and each `set` checks the bounds: `fast` first.
        strategy.set("fast", static_cast<double>(fast));
        strategy.set("slow", static_cast<double>(slow));
        return t::run(strategy, symbols, events, s);
      };
      ++cases;
      good += compare(tag("sma") + " fast=" + std::to_string(fast) + " slow=" + std::to_string(slow),
        {{"library SmaCrossover", t::run(atl::strategy::vanilla::SmaCrossover{q}, symbols, events, s)},
         {"incremental", compiled(atls::SmaIncremental(id))},
         {"naive", compiled(atls::SmaNaive(id))},
         {"exact", compiled(atls::SmaExact(id))},
         {"noshare", compiled(atls::SmaNoshare(id))}},
        comparisons);
    }

    struct PullbackPoint {
      std::int64_t trend;
      std::int64_t fast;
      double entry;
      double exit;
    };
    for (const auto& p : std::vector<PullbackPoint>{{100, 3, 20, 70}, {50, 2, 10, 80}, {200, 5, 30, 60}}) {
      const auto compiled = [&](auto strategy) {
        using S = decltype(strategy);
        typename S::Params params;
        params.trend_ = p.trend;
        params.fast_ = p.fast;
        params.entry_ = p.entry;
        params.exit_ = p.exit;
        return t::run(S(id, params), symbols, events, s);
      };
      const hand::Pullback::Params h{static_cast<std::size_t>(p.trend), static_cast<std::size_t>(p.fast), p.entry, p.exit, 100};
      ++cases;
      good += compare(tag("pullback") + " trend=" + std::to_string(p.trend) + " fast=" + std::to_string(p.fast),
        {{"hand Pullback", t::run(hand::Pullback(id, h), symbols, events, s)},
         {"incremental", compiled(atls::PullbackIncremental(id))},
         {"naive", compiled(atls::PullbackNaive(id))},
         {"exact", compiled(atls::PullbackExact(id))},
         {"noshare", compiled(atls::PullbackNoshare(id))}},
        comparisons);
    }

    for (const auto& h : std::vector<hand::TrailingTrend::Params>{{20, 60, 0.05, 120, 30, 100}, {5, 20, 0.02, 30, 5, 100}, {10, 120, 0.1, 400, 0, 100}}) {
      const auto compiled = [&](auto strategy) {
        using S = decltype(strategy);
        typename S::Params params;
        params.fast_ = static_cast<std::int64_t>(h.fast_);
        params.slow_ = static_cast<std::int64_t>(h.slow_);
        params.trail_ = h.trail_;
        params.hold_ = h.hold_;
        params.cooldown_ = h.cooldown_;
        return t::run(S(id, params), symbols, events, s);
      };
      ++cases;
      good += compare(tag("trail") + " fast=" + std::to_string(h.fast_) + " slow=" + std::to_string(h.slow_),
        {{"hand TrailingTrend", t::run(hand::TrailingTrend(id, h), symbols, events, s)},
         {"incremental", compiled(atls::TrailIncremental(id))},
         {"naive", compiled(atls::TrailNaive(id))},
         {"exact", compiled(atls::TrailExact(id))},
         {"noshare", compiled(atls::TrailNoshare(id))}},
        comparisons);
    }
  }
  std::printf("%d/%d cases identical, %d comparisons with the reference (expected %d cases)\n", good, cases, comparisons, EXPECTED_CASES);
  return good == cases && cases == EXPECTED_CASES ? 0 : 1;
}
