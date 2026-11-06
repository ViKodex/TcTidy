using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// The update check's persisted state, kept as plain strings in the extension's options: the
    /// time of the last successful check, and the ids of the dismissed notices.
    /// </summary>
    public static class UpdateSettings
    {
        /// <summary>Automatic checks run at most this often; a failed check is retried at the next load.</summary>
        public static readonly TimeSpan CheckInterval = TimeSpan.FromHours(24);

        public static string Stamp(DateTime utcNow) => utcNow.ToString("o", CultureInfo.InvariantCulture);

        /// <summary>
        /// True when no check succeeded within <see cref="CheckInterval"/>. A missing or unreadable
        /// stamp, or one in the future (the clock was turned back), is due.
        /// </summary>
        public static bool IsCheckDue(string? lastCheck, DateTime utcNow)
        {
            if (!DateTime.TryParse(lastCheck, CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind, out var last))
            {
                return true;
            }
            var elapsed = utcNow - last.ToUniversalTime();
            return elapsed < TimeSpan.Zero || elapsed >= CheckInterval;
        }

        public static IReadOnlyList<string> ParseIds(string? ids) =>
            (ids ?? "").Split(new[] { '\n' }, StringSplitOptions.RemoveEmptyEntries)
                .Select(id => id.Trim())
                .Where(id => id.Length > 0)
                .ToList();

        /// <summary>Appends <paramref name="id"/> to the newline-separated list, once.</summary>
        public static string AddId(string? ids, string id)
        {
            var list = ParseIds(ids).ToList();
            if (!list.Contains(id)) list.Add(id.Trim());
            return string.Join("\n", list);
        }
    }
}
