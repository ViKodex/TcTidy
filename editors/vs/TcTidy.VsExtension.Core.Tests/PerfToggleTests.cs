using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Pins the status-bar wording of the perf-report toggle. Pure strings, no VS needed.</summary>
    public sealed class PerfToggleTests
    {
        [Fact]
        public void StatusMessage_when_enabled_names_the_output_pane_and_the_log_file()
        {
            var message = PerfToggle.StatusMessage(enabled: true);

            Assert.StartsWith("TcTidy: performance report on", message);
            Assert.Contains("TcTidy Output pane", message);
            Assert.Contains(@"%TEMP%\tctidy-perf.log", message);
        }

        [Fact]
        public void StatusMessage_when_disabled_says_off()
        {
            Assert.Equal("TcTidy: performance report off", PerfToggle.StatusMessage(enabled: false));
        }
    }
}
