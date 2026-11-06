using System;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Exercises <see cref="UpdateSettings"/>: the check schedule and the dismissed-notice list.</summary>
    public sealed class UpdateSettingsTests
    {
        private static readonly DateTime Now = new DateTime(2026, 5, 4, 12, 0, 0, DateTimeKind.Utc);

        [Fact]
        public void A_check_is_due_once_the_interval_has_passed()
        {
            Assert.False(UpdateSettings.IsCheckDue(UpdateSettings.Stamp(Now.AddHours(-23)), Now));
            Assert.True(UpdateSettings.IsCheckDue(UpdateSettings.Stamp(Now.AddHours(-24)), Now));
        }

        [Theory]
        [InlineData(null)]
        [InlineData("")]
        [InlineData("yesterday")]
        public void A_missing_or_unreadable_stamp_is_due(string? stamp)
        {
            Assert.True(UpdateSettings.IsCheckDue(stamp, Now));
        }

        [Fact]
        public void A_stamp_in_the_future_is_due()
        {
            Assert.True(UpdateSettings.IsCheckDue(UpdateSettings.Stamp(Now.AddHours(1)), Now));
        }

        [Fact]
        public void AddId_appends_each_id_once()
        {
            var ids = UpdateSettings.AddId(null, "a");
            ids = UpdateSettings.AddId(ids, "b");
            ids = UpdateSettings.AddId(ids, "a");

            Assert.Equal(new[] { "a", "b" }, UpdateSettings.ParseIds(ids));
        }

        [Fact]
        public void ParseIds_skips_blank_lines()
        {
            Assert.Equal(new[] { "a", "b" }, UpdateSettings.ParseIds("a\n\n b \n"));
            Assert.Empty(UpdateSettings.ParseIds(null));
        }
    }
}
