// External indicators and functions whose C++ restates built-ins (user-code.hpp) give the built-ins' values, in every
// variant the generated tests use, and under restart and poison: each program buys one share more than the number of
// bars after the gate where the two differ (by atls `==`, so a zero's sign is not compared) or where only one is
// defined, so one fill of one share (per instrument) means no difference. The user's C++ throws if it is given an
// undefined value, so a run also shows that it never is.

// The user's header first: the generated headers name its classes and functions.
#include "user-code.hpp"

#include "replay.hpp"

#include <extern_std.hpp>
#include <identity_extern_exact.hpp>
#include <identity_extern_incremental.hpp>
#include <identity_extern_keep.hpp>
#include <identity_extern_missing_exact.hpp>
#include <identity_extern_missing_incremental.hpp>
#include <identity_extern_missing_keep.hpp>
#include <identity_extern_missing_naive.hpp>
#include <identity_extern_missing_noshare.hpp>
#include <identity_extern_missing_poison_exact.hpp>
#include <identity_extern_missing_poison_incremental.hpp>
#include <identity_extern_missing_poison_keep.hpp>
#include <identity_extern_missing_poison_naive.hpp>
#include <identity_extern_missing_poison_noshare.hpp>
#include <identity_extern_naive.hpp>
#include <identity_extern_noshare.hpp>
#include <identity_extern_pair_exact.hpp>
#include <identity_extern_pair_incremental.hpp>
#include <identity_extern_pair_keep.hpp>
#include <identity_extern_pair_naive.hpp>
#include <identity_extern_pair_noshare.hpp>

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

/** Each of the strategies `S`, built from `ids`, over `events` with each window: one share of each of `bought`. */
template <class... S, class Ids>
void same_bits(
  const Ids& ids, const std::vector<std::string>& symbols, const std::vector<std::string>& bought,
  const std::vector<atl::strategy::BarEvent>& events
) {
  for (const double n : {2.0, 5.0, 20.0, 90.0}) {
    INFO("n = " << n);
    (one_share_each(t::run(t::with(S(ids), {{"n", n}}), symbols, events), bought), ...);
  }
}

} // namespace

TEST_CASE("sma and least squares and functions restated as external C++ give equal values in every variant") {
  const auto events = t::daily_events({t::walk("A", 10, 2000)});
  same_bits<
    atls::IdentityExternIncremental, atls::IdentityExternNaive, atls::IdentityExternExact, atls::IdentityExternNoshare,
    atls::IdentityExternKeep
  >(t::id_of("A"), {"A"}, {"A"}, events);
}

TEST_CASE("ols_slope and ols_intercept restated as an external indicator give equal values in every variant") {
  const auto events = t::daily_events({t::walk("Y", 11, 2000), t::walk("X", 12, 2000)});
  const std::vector<atl::InstrumentId> ids{t::id_of("Y"), t::id_of("X")};
  same_bits<
    atls::IdentityExternPairIncremental, atls::IdentityExternPairNaive, atls::IdentityExternPairExact,
    atls::IdentityExternPairNoshare, atls::IdentityExternPairKeep
  >(ids, {"Y", "X"}, {"Y"}, events);
}

TEST_CASE("with missing bars external indicators under restart and poison give the values of the built-ins") {
  // C lists after 200 days; every instrument misses about one day in 10.
  const auto events =
    t::daily_events({t::walk("A", 13, 1200, 10), t::walk("B", 14, 1200, 10), t::walk("C", 15, 1200, 10, 200)});
  const std::vector<std::string> symbols{"A", "B", "C"};
  std::vector<atl::InstrumentId> ids;
  for (const auto& s : symbols) {
    ids.push_back(t::id_of(s));
  }
  same_bits<
    atls::IdentityExternMissingIncremental, atls::IdentityExternMissingNaive, atls::IdentityExternMissingExact,
    atls::IdentityExternMissingNoshare, atls::IdentityExternMissingKeep
  >(ids, symbols, symbols, events);
  same_bits<
    atls::IdentityExternMissingPoisonIncremental, atls::IdentityExternMissingPoisonNaive,
    atls::IdentityExternMissingPoisonExact, atls::IdentityExternMissingPoisonNoshare,
    atls::IdentityExternMissingPoisonKeep
  >(ids, symbols, symbols, events);
}

TEST_CASE("an external indicator named std beside an int external function compiles and trades") {
  const auto events = t::daily_events({t::walk("A", 16, 100)});
  const auto outcome = t::run(atls::ExternStd(t::id_of("A")), {"A"}, events);
  REQUIRE(outcome.result_.executions_.size() == 1);
  CHECK(outcome.result_.executions_[0].fill_.quantity_ == atl::Decimal64(3, 0));
}
