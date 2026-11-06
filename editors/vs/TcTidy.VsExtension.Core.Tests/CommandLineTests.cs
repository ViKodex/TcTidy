using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Exercises <see cref="CommandLine.Quote"/> against the C runtime's parsing rules.</summary>
    public sealed class CommandLineTests
    {
        [Theory]
        [InlineData(@"C:\Plc\FB_X.TcPOU", "\"C:\\Plc\\FB_X.TcPOU\"")]
        [InlineData(@"C:\My Solution", "\"C:\\My Solution\"")]
        // A trailing backslash would otherwise escape the closing quote.
        [InlineData(@"C:\", "\"C:\\\\\"")]
        [InlineData(@"\\server\share\", "\"\\\\server\\share\\\\\"")]
        [InlineData("say \"hi\"", "\"say \\\"hi\\\"\"")]
        [InlineData(@"a\""b", "\"a\\\\\\\"b\"")]
        [InlineData("", "\"\"")]
        public void Quote_round_trips_through_the_C_runtime_rules(string argument, string expected)
        {
            Assert.Equal(expected, CommandLine.Quote(argument));
        }
    }
}
