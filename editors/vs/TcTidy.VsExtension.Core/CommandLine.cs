using System.Text;

namespace TcTidy.VsExtension.Core
{
    /// <summary>
    /// Builds Windows command lines by hand, since netstandard2.0 has no
    /// <c>ProcessStartInfo.ArgumentList</c>.
    /// </summary>
    public static class CommandLine
    {
        /// <summary>
        /// Quote one argument so the C runtime's parser reads it back unchanged: backslashes are
        /// doubled only where they precede a quote, so a directory such as <c>C:\</c> does not turn
        /// its closing quote into a literal one.
        /// </summary>
        public static string Quote(string argument)
        {
            var quoted = new StringBuilder("\"");
            var backslashes = 0;
            foreach (var c in argument)
            {
                if (c == '\\')
                {
                    backslashes++;
                    continue;
                }
                quoted.Append('\\', c == '"' ? backslashes * 2 + 1 : backslashes);
                backslashes = 0;
                quoted.Append(c);
            }
            quoted.Append('\\', backslashes * 2);
            return quoted.Append('"').ToString();
        }
    }
}
