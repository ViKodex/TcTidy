using System;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Threading.Tasks;
using TcTidy.VsExtension.Core;
using Xunit;

namespace TcTidy.VsExtension.Core.Tests
{
    /// <summary>
    /// Exercises <see cref="SettingsServer"/> against the real tctidy binary (<c>TCTIDY_BIN</c>, as
    /// in <see cref="TctidyRunnerTests"/>): it starts, serves the page's API for its token only, and
    /// stops.
    /// </summary>
    public sealed class SettingsServerTests
    {
        private static string TctidyPath()
        {
            var p = Environment.GetEnvironmentVariable("TCTIDY_BIN");
            if (string.IsNullOrEmpty(p) || !File.Exists(p))
            {
                throw new InvalidOperationException(
                    "Set TCTIDY_BIN to a built tctidy binary to run these tests.");
            }
            return p!;
        }

        [Fact]
        public async Task Start_serves_the_settings_of_the_directory_until_disposed()
        {
            var solution = Path.Combine(Path.GetTempPath(), $"tctidy-settings-{Guid.NewGuid():N}");
            Directory.CreateDirectory(solution);
            try
            {
                File.WriteAllText(Path.Combine(solution, ".tctidy.toml"), "indent_width = 2\n");
                var server = SettingsServer.Start(TctidyPath(), solution, TimeSpan.FromSeconds(10));

                Assert.True(server.IsRunning);
                Assert.Equal(solution, server.RootDirectory);
                Assert.Matches(@"^http://127\.0\.0\.1:\d+/#[0-9a-f]{32}$", server.Url);

                var parts = server.Url.Split('#');
                using (var client = new HttpClient())
                {
                    var denied = await client.GetAsync(parts[0] + "api/state");
                    Assert.Equal(HttpStatusCode.Unauthorized, denied.StatusCode);

                    var request = new HttpRequestMessage(HttpMethod.Get, parts[0] + "api/state");
                    request.Headers.Add("X-TcTidy-Token", parts[1]);
                    var state = await client.SendAsync(request);
                    Assert.Equal(HttpStatusCode.OK, state.StatusCode);
                    var json = await state.Content.ReadAsStringAsync();
                    Assert.Contains("\"exists\":true", json);
                    Assert.Contains("\"key\":\"indent_width\"", json);
                }

                server.Dispose();
                Assert.False(server.IsRunning);
            }
            finally { Directory.Delete(solution, recursive: true); }
        }

        [Fact]
        public void Start_reports_why_tctidy_refused()
        {
            var missing = Path.Combine(Path.GetTempPath(), $"tctidy-missing-{Guid.NewGuid():N}");

            var ex = Assert.Throws<FormatterException>(
                () => SettingsServer.Start(TctidyPath(), missing, TimeSpan.FromSeconds(10)));

            Assert.Contains("not a directory", ex.Message);
        }

        [Fact]
        public void Start_reports_a_missing_binary()
        {
            var ex = Assert.Throws<FormatterException>(() => SettingsServer.Start(
                Path.Combine(Path.GetTempPath(), "tctidy-does-not-exist"), Path.GetTempPath(), TimeSpan.FromSeconds(5)));

            Assert.Contains("could not start tctidy", ex.Message);
        }
    }
}
