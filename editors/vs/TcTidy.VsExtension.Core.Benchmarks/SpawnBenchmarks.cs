using System;
using System.Collections.Generic;
using System.IO;
using BenchmarkDotNet.Attributes;
using TcTidy.VsExtension.Core;

namespace TcTidy.VsExtension.Core.Benchmarks
{
    /// <summary>
    /// The cost the VS extension pays to format a batch of files on disk, by strategy: spawning
    /// <c>tctidy</c> once <b>per file</b> (<see cref="DiskPerFileStrategy"/>, the baseline) vs once
    /// for the <b>whole batch</b> (<see cref="DiskBatchStrategy"/>, what the Format command uses).
    /// <para>
    /// Set <c>TCTIDY_BIN</c> to a built tctidy binary. Informational: the per-file ÷ batch
    /// ratio is the signal, and it dwarfs runner noise; the absolute milliseconds do not.
    /// </para>
    /// </summary>
    [SimpleJob(warmupCount: 3, iterationCount: 5)]
    public class SpawnBenchmarks
    {
        [Params(1, 8, 32)]
        public int FileCount;

        private string _dir = string.Empty;
        private List<string> _files = new();
        private DiskPerFileStrategy _perFile = null!;
        private DiskBatchStrategy _batch = null!;

        [GlobalSetup]
        public void Setup()
        {
            var bin = Environment.GetEnvironmentVariable("TCTIDY_BIN");
            if (string.IsNullOrEmpty(bin) || !File.Exists(bin))
            {
                throw new InvalidOperationException(
                    "Set TCTIDY_BIN to a built tctidy binary to run these benchmarks.");
            }

            var runner = new TctidyRunner(bin!, TimeSpan.FromSeconds(120));
            _perFile = new DiskPerFileStrategy(runner);
            _batch = new DiskBatchStrategy(runner);

            _dir = Path.Combine(Path.GetTempPath(), "tctidy-bench-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(_dir);
            _files = new List<string>(FileCount);
            for (var i = 0; i < FileCount; i++)
            {
                var path = Path.Combine(_dir, $"FB_Sample{i}.TcPOU");
                File.WriteAllText(path, SamplePou());
                _files.Add(path);
            }

            // Format once up front so every measured run is a stable no-op (read + parse +
            // format + compare, no write), isolating spawn cost from first-write churn.
            _batch.FormatAll(_files);
        }

        [GlobalCleanup]
        public void Cleanup()
        {
            try { Directory.Delete(_dir, recursive: true); }
            catch { /* best effort */ }
        }

        [Benchmark(Baseline = true)]
        public void PerFileSpawn() => _perFile.FormatAll(_files);

        [Benchmark]
        public void BatchSpawn() => _batch.FormatAll(_files);

        private static string SamplePou() =>
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n"
            + "<TcPlcObject Version=\"1.1.0.1\">\n"
            + "  <POU Name=\"FB_Sample\" Id=\"{0a0a0a0a-0000-0000-0000-000000000000}\">\n"
            + "    <Declaration><![CDATA[FUNCTION_BLOCK FB_Sample\n"
            + "VAR\n    a : INT;\n    b : LREAL := 1.0;\n    c : BOOL;\nEND_VAR]]></Declaration>\n"
            + "    <Implementation>\n"
            + "      <ST><![CDATA[IF a > 0 THEN\n    b := b + 1.0;\n    c := NOT c;\nEND_IF]]></ST>\n"
            + "    </Implementation>\n"
            + "  </POU>\n"
            + "</TcPlcObject>\n";
    }
}
