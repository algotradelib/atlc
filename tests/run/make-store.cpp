// Writes the store test's Parquet store with DuckDB: one series, `test/1d/AAA`, in two fetched versions.
// - fetched 2026-01-01: days 0 to 9 from 2020-01-01, open 100 + day, close open + 1;
// - fetched 2026-02-01: days 5 to 10, close open + 3 (days 5 to 9 restated, day 10 new).
// Read as it is, the store holds 11 bars, the last closing at 113; as of 2026-01-15, 10 bars, the last closing at 110.
#include <duckdb.h>

#include <cstdio>
#include <filesystem>
#include <string>

namespace {

/** Runs a statement; prints DuckDB's error and returns false when it fails. */
bool exec(duckdb_connection con, const std::string& sql) {
  duckdb_result result;
  const bool ok = duckdb_query(con, sql.c_str(), &result) != DuckDBError;
  if (!ok) {
    std::fprintf(stderr, "duckdb: %s\n  in: %s\n", duckdb_result_error(&result), sql.c_str());
  }
  duckdb_destroy_result(&result);
  return ok;
}

} // namespace

int main(int argc, char** argv) {
  if (argc != 2) {
    std::fprintf(stderr, "usage: make-store STORE_ROOT\n");
    return 2;
  }
  const auto dir = std::filesystem::path(argv[1]) / "bars" / "source=test" / "interval=1d" / "symbol=AAA";
  std::filesystem::create_directories(dir);
  duckdb_database db;
  duckdb_connection con;
  if (duckdb_open(nullptr, &db) == DuckDBError || duckdb_connect(db, &con) == DuckDBError) {
    std::fprintf(stderr, "cannot open DuckDB\n");
    return 1;
  }
  const std::string select =
    "SELECT 'test' AS source, '1d' AS interval, 'AAA' AS symbol, TIMESTAMPTZ '2020-01-01 00:00:00+00' + to_days(d::INTEGER) AS ts, "
    "'close' AS stamp, (100 + d)::DECIMAL(18,4) AS open, (103 + d)::DECIMAL(18,4) AS high, (99 + d)::DECIMAL(18,4) AS low, "
    "(100 + d + {up})::DECIMAL(18,4) AS close, 1000::DECIMAL(24,4) AS volume, NULL::DOUBLE AS adj_close, 'USD' AS currency, "
    "'raw' AS basis, TIMESTAMPTZ '{fetched} 00:00:00+00' AS fetched_at FROM range({from}, {to}) t(d)";
  auto version = [&select](const char* up, const char* fetched, const char* from, const char* to) {
    std::string s = select;
    for (const auto& [key, value] : {std::pair<std::string, std::string>{"{up}", up}, {"{fetched}", fetched}, {"{from}", from}, {"{to}", to}}) {
      s.replace(s.find(key), key.size(), value);
    }
    return s;
  };
  const auto path = (dir / "bars.parquet").generic_string();
  const bool ok = exec(con, "SET TimeZone = 'UTC'") &&
    exec(con, "CREATE TABLE bars AS " + version("1", "2026-01-01", "0", "10") + " UNION ALL " + version("3", "2026-02-01", "5", "11")) &&
    exec(con, "COPY (SELECT * FROM bars ORDER BY ts, fetched_at) TO '" + path + "' (FORMAT parquet)");
  duckdb_disconnect(&con);
  duckdb_close(&db);
  return ok ? 0 : 1;
}
