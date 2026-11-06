namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Status-bar wording for the TcTidy menu's Performance Report toggle. In Core so the one
    /// user-visible string that tells people where the report lands stays pinned by a test.
    /// </summary>
    public static class PerfToggle
    {
        public static string StatusMessage(bool enabled) =>
            enabled
                ? @"TcTidy: performance report on - see the TcTidy Output pane and %TEMP%\tctidy-perf.log after the next Format"
                : "TcTidy: performance report off";
    }
}
