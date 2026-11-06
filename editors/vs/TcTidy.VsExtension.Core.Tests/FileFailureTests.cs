using System;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    public sealed class FileFailureTests
    {
        [Fact]
        public void ToString_renders_source_then_message()
        {
            var failure = new FileFailure(@"C:\plc\POUs\FB_X.TcPOU", "1:3: parse error");
            Assert.Equal(@"C:\plc\POUs\FB_X.TcPOU: 1:3: parse error", failure.ToString());
        }

        [Fact]
        public void A_null_source_is_rejected()
        {
            Assert.Throws<ArgumentNullException>(() => new FileFailure(null!, "message"));
        }

        [Fact]
        public void A_null_message_is_rejected()
        {
            Assert.Throws<ArgumentNullException>(() => new FileFailure("source", null!));
        }
    }
}
