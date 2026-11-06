using System.Collections.Generic;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>Exercises <see cref="SettingsPageHost"/> with a fake server: one server per IDE.</summary>
    public sealed class SettingsPageHostTests
    {
        [Fact]
        public void Reopening_the_same_solution_reuses_the_running_server()
        {
            var started = new List<FakeServer>();
            using var host = Host(started);

            var first = host.Open("tctidy", "/work/Project");
            var again = host.Open("tctidy", "/work/Project/");

            Assert.Single(started);
            Assert.Equal(first, again);
        }

        [Fact]
        public void A_server_that_stopped_on_its_own_is_started_again()
        {
            var started = new List<FakeServer>();
            using var host = Host(started);

            host.Open("tctidy", "/work/Project");
            started[0].IsRunning = false;
            var url = host.Open("tctidy", "/work/Project");

            Assert.Equal(2, started.Count);
            Assert.Equal(started[1].Url, url);
        }

        [Fact]
        public void Another_solution_stops_the_previous_server()
        {
            var started = new List<FakeServer>();
            using var host = Host(started);

            host.Open("tctidy", "/work/Project");
            host.Open("tctidy", "/work/OtherProject");

            Assert.Equal(2, started.Count);
            Assert.True(started[0].Disposed);
            Assert.Equal("/work/OtherProject", started[1].RootDirectory);
        }

        [Fact]
        public void Stop_and_dispose_stop_the_running_server()
        {
            var started = new List<FakeServer>();
            var host = Host(started);

            host.Open("tctidy", "/work/Project");
            host.Stop();
            Assert.True(started[0].Disposed);

            host.Open("tctidy", "/work/Project");
            host.Dispose();
            Assert.True(started[1].Disposed);
        }

        [Fact]
        public void A_failed_start_leaves_no_server_behind()
        {
            var host = new SettingsPageHost((exe, root) => throw new FormatterException(-1, "no such directory"));

            var ex = Assert.Throws<FormatterException>(() => host.Open("tctidy", "/work/Project"));
            Assert.Equal("no such directory", ex.Message);
            host.Dispose();
        }

        private static SettingsPageHost Host(List<FakeServer> started) =>
            new SettingsPageHost((exe, root) =>
            {
                var server = new FakeServer(root, $"http://127.0.0.1:{5000 + started.Count}/#token");
                started.Add(server);
                return server;
            });

        private sealed class FakeServer : ISettingsServer
        {
            public FakeServer(string root, string url)
            {
                RootDirectory = root;
                Url = url;
            }

            public string Url { get; }
            public string RootDirectory { get; }
            public bool IsRunning { get; set; } = true;
            public bool Disposed { get; private set; }

            public void Dispose()
            {
                Disposed = true;
                IsRunning = false;
            }
        }
    }
}
