# frozen_string_literal: true

# The Exception object set by Kernel#raise.
$! = _

# The array contains the module names loaded by require.
$" = _

# The process number of the Ruby running this script. Same as Process.pid.
$$ = _

# The string matched by the last successful match.
$& = _

# The string to the right of the last successful match.
$' = _

# The same as ARGV.
$* = _

# The highest group matched by the last successful match.
$+ = _

# The output field separator for Kernel#print and Array#join. Non-nil $, will be deprecated.
$, = _

# The input record separator, newline by default.
$-0 = _

# The default separator for String#split. Non-nil $; will be deprecated.
$-F = _

# Load path for searching Ruby scripts and extension libraries used by Kernel#load and Kernel#require.  Has a singleton method
# <code>$LOAD_PATH.resolve_feature_path(feature)</code> that returns [+:rb+ or +:so+, path], which resolves the feature to the path the
# original Kernel#require method would load.
$-I = _

$-W = _

# True if option <tt>-a</tt> is set. Read-only variable.
$-a = _

# The debug flag, which is set by the <tt>-d</tt> switch.  Enabling debug output prints each exception raised to $stderr (but not its
# backtrace).  Setting this to a true value enables debug output as if <tt>-d</tt> were given on the command line.  Setting this to a false
# value disables debug output.
$-d = _

# In in-place-edit mode, this variable holds the extension, otherwise +nil+.
$-i = _

# True if option <tt>-l</tt> is set. Read-only variable.
$-l = _

# True if option <tt>-p</tt> is set. Read-only variable.
$-p = _

# The verbose flag, which is set by the <tt>-w</tt> or <tt>-v</tt> switch.  Setting this to a true value enables warnings as if <tt>-w</tt>
# or <tt>-v</tt> were given on the command line.  Setting this to +nil+ disables warnings, including from Kernel#warn.
$-v = _

# The verbose flag, which is set by the <tt>-w</tt> or <tt>-v</tt> switch.  Setting this to a true value enables warnings as if <tt>-w</tt>
# or <tt>-v</tt> were given on the command line.  Setting this to +nil+ disables warnings, including from Kernel#warn.
$-w = _

# The current input line number of the last file that was read.
$. = _

# The input record separator, newline by default. Aliased to $-0.
$/ = _

# Contains the name of the script being executed. May be assignable.
$0 = _

# The Nth group of the last successful match. May be > 1.
$1 = _

# The Nth group of the last successful match. May be > 1.
$2 = _

# The Nth group of the last successful match. May be > 1.
$3 = _

# The Nth group of the last successful match. May be > 1.
$4 = _

# The Nth group of the last successful match. May be > 1.
$5 = _

# The Nth group of the last successful match. May be > 1.
$6 = _

# The Nth group of the last successful match. May be > 1.
$7 = _

# The Nth group of the last successful match. May be > 1.
$8 = _

# The Nth group of the last successful match. May be > 1.
$9 = _

# Load path for searching Ruby scripts and extension libraries used by Kernel#load and Kernel#require.  Has a singleton method
# <code>$LOAD_PATH.resolve_feature_path(feature)</code> that returns [+:rb+ or +:so+, path], which resolves the feature to the path the
# original Kernel#require method would load.
$: = _

# The default separator for String#split. Non-nil $; will be deprecated. Aliased to $-F.
$; = _

# The same as ARGF.
$< = _

# This variable is no longer effective. Deprecated.
$= = _

# The default output stream for Kernel#print and Kernel#printf. $stdout by default.
$> = _

# The status of the last executed child process (thread-local).
$? = _

# The same as <code>$!.backtrace</code>.
$@ = _

# The debug flag, which is set by the <tt>-d</tt> switch.  Enabling debug output prints each exception raised to $stderr (but not its
# backtrace).  Setting this to a true value enables debug output as if <tt>-d</tt> were given on the command line.  Setting this to a false
# value disables debug output. Aliased to $-d.
$DEBUG = _

# Current input filename from ARGF. Same as ARGF.filename.
$FILENAME = _

# The array contains the module names loaded by require.
$LOADED_FEATURES = _

# Load path for searching Ruby scripts and extension libraries used by Kernel#load and Kernel#require. Aliased to $: and $-I.  Has a
# singleton method <code>$LOAD_PATH.resolve_feature_path(feature)</code> that returns [+:rb+ or +:so+, path], which resolves the feature to
# the path the original Kernel#require method would load.
$LOAD_PATH = _

# Contains the name of the script being executed. May be assignable.
$PROGRAM_NAME = _

# The verbose flag, which is set by the <tt>-w</tt> or <tt>-v</tt> switch.  Setting this to a true value enables warnings as if <tt>-w</tt>
# or <tt>-v</tt> were given on the command line.  Setting this to +nil+ disables warnings, including from Kernel#warn. Aliased to $-v and $-w.
$VERBOSE = _

# The output record separator for Kernel#print and IO#write. Default is +nil+.
$\ = _

# The last input line of string by gets or readline.
$_ = _

# The string to the left of the last successful match.
$` = _

# The current standard error output.
$stderr = _

# The current standard input.
$stdin = _

# The current standard output.
$stdout = _

# The information about the last match in the current scope (thread-local and frame-local).
$~ = _
