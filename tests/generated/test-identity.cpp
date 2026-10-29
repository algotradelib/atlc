// User indicators that restate built-ins (tests/generated/programs/) give the built-ins' bits on every bar: each
// program buys one share more than the number of bars where the two differ, so one fill of one share (per
// instrument) means no difference. Inputs are prices divided by 7 or returns, which a double cannot hold exactly,
// so floating-point contraction on one side alone would show.
#include "replay.hpp"

#include <identity.hpp>
#include <identity_missing.hpp>
#include <identity_missing_poison.hpp>
#include <identity_pair.hpp>
#include <identity_perturbed.hpp>

#include <catch2/catch_test_macros.hpp>

namespace {

namespace atl = algotradelib;
namespace t = atlc_test;

/** One fill of one share on each of `symbols`, and nothing else. */
void one_share_each(const t::Outcome& outcome, const std::vector<std::string>& symbols) {
  REQUIRE(outcome.result_.executions_.size() == symbols.size());
  for (std::size_t k = 0; k < symbols.size(); ++k) {
    const auto& fill = outcome.result_.executions_[k].fill_;
    CHECK(fill.quantity_ == atl::Decimal64(1, 0));
    CHECK(fill.side_ == atl::OrderSide::BUY);
  }
}

} // namespace

TEST_CASE("sma, stdev and zscore restated as user indicators give the same bits") {
  const auto events = t::daily_events({t::walk("A", 10, 2000)});
  for (const double n : {2.0, 5.0, 20.0, 90.0}) {
    INFO("n = " << n);
    one_share_each(t::run(t::with(atls::Identity(t::id_of("A")), {{"n", n}}), {"A"}, events), {"A"});
  }
}

TEST_CASE("the identity check sees a difference of one part in 1e15") {
  // identity.atls with the user mean multiplied by 1 + 1e-15.
  const auto events = t::daily_events({t::walk("A", 10, 2000)});
  const auto outcome = t::run(atls::IdentityPerturbed(t::id_of("A")), {"A"}, events);
  CHECK(outcome.result_.executions_.size() > 100);
}

TEST_CASE("ols_slope and ols_intercept restated over a window of pairs give the same bits") {
  const std::vector<t::Series> all{t::walk("Y", 11, 2000), t::walk("X", 12, 2000)};
  const auto events = t::daily_events(all);
  const std::vector<atl::InstrumentId> ids{t::id_of("Y"), t::id_of("X")};
  for (const double n : {2.0, 5.0, 20.0, 90.0}) {
    INFO("n = " << n);
    one_share_each(t::run(t::with(atls::IdentityPair(ids), {{"n", n}}), {"Y", "X"}, events), {"Y"});
  }
}

TEST_CASE("under skip and poison, with missing bars, user indicators give the built-ins' bits") {
  // C lists after 200 days; every instrument misses about one day in 10.
  const std::vector<t::Series> all{t::walk("A", 13, 1200, 10), t::walk("B", 14, 1200, 10), t::walk("C", 15, 1200, 10, 200)};
  const auto events = t::daily_events(all);
  const std::vector<std::string> symbols{"A", "B", "C"};
  std::vector<atl::InstrumentId> ids;
  for (const auto& s : symbols) {
    ids.push_back(t::id_of(s));
  }
  for (const double n : {2.0, 20.0, 90.0}) {
    INFO("n = " << n);
    one_share_each(t::run(t::with(atls::IdentityMissing(ids), {{"n", n}}), symbols, events), symbols);
    one_share_each(t::run(t::with(atls::IdentityMissingPoison(ids), {{"n", n}}), symbols, events), symbols);
  }
}
