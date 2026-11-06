using BenchmarkDotNet.Running;

namespace TcTidy.VsExtension.Core.Benchmarks
{
    public static class Program
    {
        // BenchmarkSwitcher lets CI pass `--filter '*'` and `--job short` (or `--job dry`
        // for a smoke run) from the command line.
        public static void Main(string[] args) =>
            BenchmarkSwitcher.FromAssembly(typeof(Program).Assembly).Run(args);
    }
}
