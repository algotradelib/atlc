// The run's argument rules (atls/args.hpp) against hand-written strategies with a generated strategy's interface:
// the symbols by mode, the parameters' two passes, and the pricing. Included first and alone, so the header is
// shown to need nothing a generated strategy would bring.
#include <atls/args.hpp>

#include <catch2/catch_test_macros.hpp>

#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace {

namespace run = atls::run;
namespace atl = algotradelib;

run::Settings settings(std::vector<std::string> args) {
  std::vector<char*> argv{const_cast<char*>("run")};
  for (auto& arg : args) {
    argv.push_back(arg.data());
  }
  return run::Settings::from_args(static_cast<int>(argv.size()), argv.data());
}

struct Named {
  static constexpr const char* atls_mode = "named";
  static std::vector<std::string> atls_instruments() { return {"long", "short"}; }
};

struct Single {
  static constexpr const char* atls_mode = "single";
  static std::vector<std::string> atls_instruments() { return {}; }
};

struct Universe {
  static constexpr const char* atls_mode = "universe";
  static std::vector<std::string> atls_instruments() { return {}; }
};

// Two parameters with a generated strategy's `set`: the value is stored, then every bound is checked against the
// values so far (`exit` at most `entry`, both in [0, 100]); every call is recorded, refused or not.
struct Bounded {
  double entry_ = 50;
  double exit_ = 10;
  std::vector<std::pair<std::string, double>> calls_;
  std::vector<std::pair<atl::OrderType, std::string>> pricing_;
  bool refuse_pricing_ = false;

  void set(std::string_view name, double value) {
    calls_.emplace_back(std::string(name), value);
    if (name == "entry") {
      entry_ = value;
    } else if (name == "exit") {
      exit_ = value;
    } else {
      throw std::invalid_argument("the program has no parameter '" + std::string(name) + "'");
    }
    if (entry_ < 0 || entry_ > 100 || exit_ < 0 || exit_ > entry_) {
      throw std::invalid_argument("a bound is broken");
    }
  }

  void set_order_pricing(atl::OrderType type, const atl::Decimal64& offset) {
    if (refuse_pricing_) {
      throw std::invalid_argument("the pricing is refused");
    }
    pricing_.emplace_back(type, offset.str());
  }
};

} // namespace

TEST_CASE("args: settings read key=value, later values winning") {
  const auto s = settings({"a=1", "b=x=y", "a=2", "c="});
  CHECK(s.get("a") == "2");
  CHECK(s.get("b") == "x=y");
  CHECK(s.get("c") == "");
  CHECK_FALSE(s.get("d").has_value());
  CHECK_THROWS_AS(settings({"novalue"}), std::invalid_argument);
}

TEST_CASE("args: a named program takes each declared instrument, in declaration order") {
  CHECK(run::symbols_of<Named>(settings({"instrument.short=B", "instrument.long=A", "instrument.extra=C"})) ==
        std::vector<std::string>{"A", "B"});
  CHECK(run::symbols_of<Named>(settings({"instrument.long=A", "instrument.short=B", "instrument.long=Z"})) ==
        std::vector<std::string>{"Z", "B"});
  try {
    run::symbols_of<Named>(settings({"instrument.long=A"}));
    FAIL("a missing instrument was accepted");
  } catch (const std::invalid_argument& e) {
    CHECK(std::string(e.what()) == "the run gives no symbol for instrument 'short' (instruments.short = \"...\")");
  }
}

TEST_CASE("args: a single program takes at most one instrument, none meaning the data's") {
  CHECK(run::symbols_of<Single>(settings({})).empty());
  CHECK(run::symbols_of<Single>(settings({"instrument.x=A"})) == std::vector<std::string>{"A"});
  CHECK(run::symbols_of<Single>(settings({"instrument.x=A", "instrument.x=B"})) == std::vector<std::string>{"B"});
  try {
    run::symbols_of<Single>(settings({"instrument.x=A", "instrument.y=B"}));
    FAIL("two instruments were accepted");
  } catch (const std::invalid_argument& e) {
    CHECK(std::string(e.what()) == "a single-instrument program takes one entry in [instruments]");
  }
}

TEST_CASE("args: a universe takes every instrument in order of first appearance") {
  CHECK(run::symbols_of<Universe>(settings({"instrument.b=B", "instrument.a=A", "instrument.b=C"})) ==
        std::vector<std::string>{"C", "A"});
  CHECK(run::symbols_of<Universe>(settings({"other=1"})).empty());
}

TEST_CASE("args: parameters are set twice, in argument order, the second pass checking the final values") {
  // Raising both: exit=80 alone breaks `exit <= entry` (entry is 50) until entry=90 is set.
  Bounded strategy;
  run::set_params(strategy, settings({"param.exit=80", "param.entry=90"}));
  CHECK(strategy.entry_ == 90);
  CHECK(strategy.exit_ == 80);
  const std::vector<std::pair<std::string, double>> expected{{"exit", 80}, {"entry", 90}, {"exit", 80}, {"entry", 90}};
  CHECK(strategy.calls_ == expected);
}

TEST_CASE("args: a duplicate parameter keeps its last value at its first position") {
  Bounded strategy;
  run::set_params(strategy, settings({"param.entry=60", "param.exit=5", "param.entry=70"}));
  const std::vector<std::pair<std::string, double>> expected{{"entry", 70}, {"exit", 5}, {"entry", 70}, {"exit", 5}};
  CHECK(strategy.calls_ == expected);
}

TEST_CASE("args: a bound still broken after both passes, or an unknown name, is refused by the second pass") {
  Bounded broken;
  CHECK_THROWS_AS(run::set_params(broken, settings({"param.entry=200"})), std::invalid_argument);
  CHECK(broken.calls_.size() == 2);
  Bounded unknown;
  try {
    run::set_params(unknown, settings({"param.nope=1"}));
    FAIL("an unknown parameter was accepted");
  } catch (const std::invalid_argument& e) {
    CHECK(std::string(e.what()) == "the program has no parameter 'nope'");
  }
  CHECK(unknown.calls_.size() == 2);
}

TEST_CASE("args: a value that does not parse is refused before any parameter is set") {
  Bounded junk;
  try {
    run::set_params(junk, settings({"param.entry=60", "param.exit=1x"}));
    FAIL("a value with trailing text was accepted");
  } catch (const std::invalid_argument& e) {
    CHECK(std::string(e.what()) == "param.exit: '1x' is not a number");
  }
  CHECK(junk.calls_.empty());
  // std::stod's own exceptions, as before: not a number at all, and too large for a double.
  Bounded empty;
  CHECK_THROWS_AS(run::set_params(empty, settings({"param.entry=60", "param.exit="})), std::invalid_argument);
  CHECK(empty.calls_.empty());
  Bounded huge;
  CHECK_THROWS_AS(run::set_params(huge, settings({"param.entry=1e999"})), std::out_of_range);
  CHECK(huge.calls_.empty());
}

TEST_CASE("args: the pricing defaults to market at zero, and is set as given") {
  Bounded plain;
  run::set_pricing(plain, settings({}));
  REQUIRE(plain.pricing_.size() == 1);
  CHECK(plain.pricing_[0].first == atl::OrderType::MARKET);
  CHECK(plain.pricing_[0].second == atl::Decimal64::parse("0").str());
  Bounded limit;
  run::set_pricing(limit, settings({"orders.type=limit", "orders.offset=0.01"}));
  REQUIRE(limit.pricing_.size() == 1);
  CHECK(limit.pricing_[0].first == atl::OrderType::LIMIT);
  CHECK(limit.pricing_[0].second == atl::Decimal64::parse("0.01").str());
}

TEST_CASE("args: a pricing that does not parse, or that the strategy refuses, is an error") {
  Bounded type;
  try {
    run::set_pricing(type, settings({"orders.type=stop"}));
    FAIL("an unknown order type was accepted");
  } catch (const std::invalid_argument& e) {
    CHECK(std::string(e.what()) == "orders.type is market or limit, not 'stop'");
  }
  Bounded offset;
  try {
    run::set_pricing(offset, settings({"orders.type=limit", "orders.offset=abc"}));
    FAIL("a bad offset was accepted");
  } catch (const std::invalid_argument& e) {
    CHECK(std::string(e.what()).rfind("orders.offset: 'abc' is not a decimal", 0) == 0);
  }
  Bounded refused;
  refused.refuse_pricing_ = true;
  try {
    run::set_pricing(refused, settings({}));
    FAIL("a refused pricing was accepted");
  } catch (const std::invalid_argument& e) {
    CHECK(std::string(e.what()) == "the pricing is refused");
  }
}
