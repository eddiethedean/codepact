from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

import capture_phase07_performance as benchmark


class PerformanceComparisonTest(unittest.TestCase):
    def test_process_order_and_paired_statistics(self) -> None:
        # The second pair runs candidate first. Its larger absolute change
        # and smaller relative change distinguish the two reported medians.
        values = [1.0, 1.5, 11.0, 10.0]
        arms = ["baseline", "candidate", "candidate", "baseline"]
        fields = ["cold_ms", "warm_ms", "affected_edit_ms", "unrelated_edit_ms"]
        records = [
            {
                "command": [arm, "exclusive", "250"],
                "peak_rss_bytes": index + 1,
                "raw": {field: [value] * 6 + [1000.0] for field in fields},
            }
            for index, (arm, value) in enumerate(zip(arms, values, strict=True))
        ]
        with (
            patch.object(benchmark, "PROCESS_REPEATS", 2),
            patch.object(benchmark, "measure_once", side_effect=records) as measure,
        ):
            result = benchmark.measure_comparison(
                Path("baseline"), Path("candidate"), "exclusive"
            )
        self.assertEqual([str(call.args[0]) for call in measure.call_args_list], arms)
        self.assertEqual(result["pair_order"], [arms[:2], arms[2:]])
        self.assertEqual(result["baseline"]["replicates"], [records[0], records[3]])
        self.assertEqual(result["candidate"]["peak_rss_bytes"], 3)
        self.assertEqual(result["baseline"]["median_ms"]["unchanged_session"], 10.0)
        self.assertEqual(
            result["paired_change"]["unchanged_session"],
            {
                "median_delta_us": 750.0,
                "median_delta_percent": 30.0,
                "candidate_slower_pairs": 2,
            },
        )

    def test_zero_reference_preserves_absolute_change(self) -> None:
        result = benchmark.paired_change(
            [{"raw": {"warm_ms": [0.0]}}], [{"raw": {"warm_ms": [0.001]}}], "warm_ms"
        )
        self.assertIsNone(result["median_delta_percent"])
        self.assertEqual(result["median_delta_us"], 1.0)
