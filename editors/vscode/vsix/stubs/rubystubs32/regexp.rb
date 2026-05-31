# frozen_string_literal: true

# Regular expressions (<i>regexp</i>s) are patterns which describe the
# contents of a string. They're used for testing whether a string contains a
# given pattern, or extracting the portions that match. They are created
# with the <tt>/</tt><i>pat</i><tt>/</tt> and
# <tt>%r{</tt><i>pat</i><tt>}</tt> literals or the <tt>Regexp.new</tt>
# constructor.
#
# A regexp is usually delimited with forward slashes (<tt>/</tt>). For
# example:
#
#     /hay/ =~ 'haystack'   #=> 0
#     /y/.match('haystack') #=> #<MatchData "y">
#
# If a string contains the pattern it is said to <i>match</i>. A literal
# string matches itself.
#
# Here 'haystack' does not contain the pattern 'needle', so it doesn't match:
#
#     /needle/.match('haystack') #=> nil
#
# Here 'haystack' contains the pattern 'hay', so it matches:
#
#     /hay/.match('haystack')    #=> #<MatchData "hay">
#
# Specifically, <tt>/st/</tt> requires that the string contains the letter
# _s_ followed by the letter _t_, so it matches _haystack_, also.
#
# Note that any Regexp matching will raise a RuntimeError if timeout is set and
# exceeded. See {"Timeout"}[#label-Timeout] section in detail.
#
# == \Regexp Interpolation
#
# A regexp may contain interpolated strings; trivially:
#
#   foo = 'bar'
#   /#{foo}/ # => /bar/
#
# == <tt>=~</tt> and Regexp#match
#
# Pattern matching may be achieved by using <tt>=~</tt> operator or Regexp#match
# method.
#
# === <tt>=~</tt> Operator
#
# <tt>=~</tt> is Ruby's basic pattern-matching operator.  When one operand is a
# regular expression and the other is a string then the regular expression is
# used as a pattern to match against the string.  (This operator is equivalently
# defined by Regexp and String so the order of String and Regexp do not matter.
# Other classes may have different implementations of <tt>=~</tt>.)  If a match
# is found, the operator returns index of first match in string, otherwise it
# returns +nil+.
#
#     /hay/ =~ 'haystack'   #=> 0
#     'haystack' =~ /hay/   #=> 0
#     /a/   =~ 'haystack'   #=> 1
#     /u/   =~ 'haystack'   #=> nil
#
# Using <tt>=~</tt> operator with a String and Regexp the <tt>$~</tt> global
# variable is set after a successful match.  <tt>$~</tt> holds a MatchData
# object. Regexp.last_match is equivalent to <tt>$~</tt>.
#
# === Regexp#match Method
#
# The #match method returns a MatchData object:
#
#     /st/.match('haystack')   #=> #<MatchData "st">
#
# == Metacharacters and Escapes
#
# The following are <i>metacharacters</i> <tt>(</tt>, <tt>)</tt>,
# <tt>[</tt>, <tt>]</tt>, <tt>{</tt>, <tt>}</tt>, <tt>.</tt>, <tt>?</tt>,
# <tt>+</tt>, <tt>*</tt>. They have a specific meaning when appearing in a
# pattern. To match them literally they must be backslash-escaped. To match
# a backslash literally, backslash-escape it: <tt>\\\\</tt>.
#
#     /1 \+ 2 = 3\?/.match('Does 1 + 2 = 3?') #=> #<MatchData "1 + 2 = 3?">
#     /a\\\\b/.match('a\\\\b')                    #=> #<MatchData "a\\b">
#
# Patterns behave like double-quoted strings and can contain the same
# backslash escapes (the meaning of <tt>\s</tt> is different, however,
# see below[#label-Character+Classes]).
#
#     /\s\u{6771 4eac 90fd}/.match("Go to 東京都")
#         #=> #<MatchData " 東京都">
#
# Arbitrary Ruby expressions can be embedded into patterns with the
# <tt>#{...}</tt> construct.
#
#     place = "東京都"
#     /#{place}/.match("Go to 東京都")
#         #=> #<MatchData "東京都">
#
# == Character Classes
#
# A <i>character class</i> is delimited with square brackets (<tt>[</tt>,
# <tt>]</tt>) and lists characters that may appear at that point in the
# match. <tt>/[ab]/</tt> means _a_ or _b_, as opposed to <tt>/ab/</tt> which
# means _a_ followed by _b_.
#
#     /W[aeiou]rd/.match("Word") #=> #<MatchData "Word">
#
# Within a character class the hyphen (<tt>-</tt>) is a metacharacter
# denoting an inclusive range of characters. <tt>[abcd]</tt> is equivalent
# to <tt>[a-d]</tt>. A range can be followed by another range, so
# <tt>[abcdwxyz]</tt> is equivalent to <tt>[a-dw-z]</tt>. The order in which
# ranges or individual characters appear inside a character class is
# irrelevant.
#
#     /[0-9a-f]/.match('9f') #=> #<MatchData "9">
#     /[9f]/.match('9f')     #=> #<MatchData "9">
#
# If the first character of a character class is a caret (<tt>^</tt>) the
# class is inverted: it matches any character _except_ those named.
#
#     /[^a-eg-z]/.match('f') #=> #<MatchData "f">
#
# A character class may contain another character class. By itself this
# isn't useful because <tt>[a-z[0-9]]</tt> describes the same set as
# <tt>[a-z0-9]</tt>. However, character classes also support the <tt>&&</tt>
# operator which performs set intersection on its arguments. The two can be
# combined as follows:
#
#     /[a-w&&[^c-g]z]/ # ([a-w] AND ([^c-g] OR z))
#
# This is equivalent to:
#
#     /[abh-w]/
#
# The following metacharacters also behave like character classes:
#
# * <tt>/./</tt> - Any character except a newline.
# * <tt>/./m</tt> - Any character (the +m+ modifier enables multiline mode)
# * <tt>/\w/</tt> - A word character (<tt>[a-zA-Z0-9_]</tt>)
# * <tt>/\W/</tt> - A non-word character (<tt>[^a-zA-Z0-9_]</tt>).
#   Please take a look at {Bug #4044}[https://bugs.ruby-lang.org/issues/4044] if
#   using <tt>/\W/</tt> with the <tt>/i</tt> modifier.
# * <tt>/\d/</tt> - A digit character (<tt>[0-9]</tt>)
# * <tt>/\D/</tt> - A non-digit character (<tt>[^0-9]</tt>)
# * <tt>/\h/</tt> - A hexdigit character (<tt>[0-9a-fA-F]</tt>)
# * <tt>/\H/</tt> - A non-hexdigit character (<tt>[^0-9a-fA-F]</tt>)
# * <tt>/\s/</tt> - A whitespace character: <tt>/[ \t\r\n\f\v]/</tt>
# * <tt>/\S/</tt> - A non-whitespace character: <tt>/[^ \t\r\n\f\v]/</tt>
# * <tt>/\R/</tt> - A linebreak: <tt>\n</tt>, <tt>\v</tt>, <tt>\f</tt>, <tt>\r</tt>
#   <tt>\u0085</tt> (NEXT LINE), <tt>\u2028</tt> (LINE SEPARATOR), <tt>\u2029</tt> (PARAGRAPH SEPARATOR)
#   or <tt>\r\n</tt>.
#
# POSIX <i>bracket expressions</i> are also similar to character classes.
# They provide a portable alternative to the above, with the added benefit
# that they encompass non-ASCII characters. For instance, <tt>/\d/</tt>
# matches only the ASCII decimal digits (0-9); whereas <tt>/[[:digit:]]/</tt>
# matches any character in the Unicode _Nd_ category.
#
# * <tt>/[[:alnum:]]/</tt> - Alphabetic and numeric character
# * <tt>/[[:alpha:]]/</tt> - Alphabetic character
# * <tt>/[[:blank:]]/</tt> - Space or tab
# * <tt>/[[:cntrl:]]/</tt> - Control character
# * <tt>/[[:digit:]]/</tt> - Digit
# * <tt>/[[:graph:]]/</tt> - Non-blank character (excludes spaces, control
#   characters, and similar)
# * <tt>/[[:lower:]]/</tt> - Lowercase alphabetical character
# * <tt>/[[:print:]]/</tt> - Like [:graph:], but includes the space character
# * <tt>/[[:punct:]]/</tt> - Punctuation character
# * <tt>/[[:space:]]/</tt> - Whitespace character (<tt>[:blank:]</tt>, newline,
#   carriage return, etc.)
# * <tt>/[[:upper:]]/</tt> - Uppercase alphabetical
# * <tt>/[[:xdigit:]]/</tt> - Digit allowed in a hexadecimal number (i.e.,
#   0-9a-fA-F)
#
# Ruby also supports the following non-POSIX character classes:
#
# * <tt>/[[:word:]]/</tt> - A character in one of the following Unicode
#   general categories _Letter_, _Mark_, _Number_,
#   <i>Connector_Punctuation</i>
# * <tt>/[[:ascii:]]/</tt> - A character in the ASCII character set
#
#     # U+06F2 is "EXTENDED ARABIC-INDIC DIGIT TWO"
#     /[[:digit:]]/.match("\u06F2")    #=> #<MatchData "\u{06F2}">
#     /[[:upper:]][[:lower:]]/.match("Hello") #=> #<MatchData "He">
#     /[[:xdigit:]][[:xdigit:]]/.match("A6")  #=> #<MatchData "A6">
#
# == Repetition
#
# The constructs described so far match a single character. They can be
# followed by a repetition metacharacter to specify how many times they need
# to occur. Such metacharacters are called <i>quantifiers</i>.
#
# * <tt>*</tt> - Zero or more times
# * <tt>+</tt> - One or more times
# * <tt>?</tt> - Zero or one times (optional)
# * <tt>{</tt><i>n</i><tt>}</tt> - Exactly <i>n</i> times
# * <tt>{</tt><i>n</i><tt>,}</tt> - <i>n</i> or more times
# * <tt>{,</tt><i>m</i><tt>}</tt> - <i>m</i> or less times
# * <tt>{</tt><i>n</i><tt>,</tt><i>m</i><tt>}</tt> - At least <i>n</i> and
#   at most <i>m</i> times
#
# At least one uppercase character ('H'), at least one lowercase character
# ('e'), two 'l' characters, then one 'o':
#
#     "Hello".match(/[[:upper:]]+[[:lower:]]+l{2}o/) #=> #<MatchData "Hello">
#
# === Greedy Match
#
# Repetition is <i>greedy</i> by default: as many occurrences as possible
# are matched while still allowing the overall match to succeed. By
# contrast, <i>lazy</i> matching makes the minimal amount of matches
# necessary for overall success. Most greedy metacharacters can be made lazy
# by following them with <tt>?</tt>. For the <tt>{n}</tt> pattern, because
# it specifies an exact number of characters to match and not a variable
# number of characters, the <tt>?</tt> metacharacter instead makes the
# repeated pattern optional.
#
# Both patterns below match the string. The first uses a greedy quantifier so
# '.+' matches '<a><b>'; the second uses a lazy quantifier so '.+?' matches
# '<a>':
#
#     /<.+>/.match("<a><b>")  #=> #<MatchData "<a><b>">
#     /<.+?>/.match("<a><b>") #=> #<MatchData "<a>">
#
# === Possessive Match
#
# A quantifier followed by <tt>+</tt> matches <i>possessively</i>: once it
# has matched it does not backtrack. They behave like greedy quantifiers,
# but having matched they refuse to "give up" their match even if this
# jeopardises the overall match.
#
#     /<.*><.+>/.match("<a><b>") #=> #<MatchData "<a><b>">
#     /<.*+><.+>/.match("<a><b>") #=> nil
#     /<.*><.++>/.match("<a><b>") #=> nil
#
# == Capturing
#
# Parentheses can be used for <i>capturing</i>. The text enclosed by the
# <i>n</i>th group of parentheses can be subsequently referred to
# with <i>n</i>. Within a pattern use the <i>backreference</i>
# <tt>\n</tt> (e.g. <tt>\1</tt>); outside of the pattern use
# <tt>MatchData[n]</tt> (e.g. <tt>MatchData[1]</tt>).
#
# In this example, <tt>'at'</tt> is captured by the first group of
# parentheses, then referred to later with <tt>\1</tt>:
#
#     /[csh](..) [csh]\1 in/.match("The cat sat in the hat")
#         #=> #<MatchData "cat sat in" 1:"at">
#
# Regexp#match returns a MatchData object which makes the captured text
# available with its #[] method:
#
#     /[csh](..) [csh]\1 in/.match("The cat sat in the hat")[1] #=> 'at'
#
# While Ruby supports an arbitrary number of numbered captured groups,
# only groups 1-9 are supported using the <tt>\n</tt> backreference
# syntax.
#
# Ruby also supports <tt>\0</tt> as a special backreference, which
# references the entire matched string.  This is also available at
# <tt>MatchData[0]</tt>.  Note that the <tt>\0</tt> backreference cannot
# be used inside the regexp, as backreferences can only be used after the
# end of the capture group, and the <tt>\0</tt> backreference uses the
# implicit capture group of the entire match.  However, you can use
# this backreference when doing substitution:
#
#   "The cat sat in the hat".gsub(/[csh]at/, '\0s')
#     # => "The cats sats in the hats"
#
# === Named Captures
#
# Capture groups can be referred to by name when defined with the
# <tt>(?<</tt><i>name</i><tt>>)</tt> or <tt>(?'</tt><i>name</i><tt>')</tt>
# constructs.
#
#     /\$(?<dollars>\d+)\.(?<cents>\d+)/.match("$3.67")
#         #=> #<MatchData "$3.67" dollars:"3" cents:"67">
#     /\$(?<dollars>\d+)\.(?<cents>\d+)/.match("$3.67")[:dollars] #=> "3"
#
# Named groups can be backreferenced with <tt>\k<</tt><i>name</i><tt>></tt>,
# where _name_ is the group name.
#
#     /(?<vowel>[aeiou]).\k<vowel>.\k<vowel>/.match('ototomy')
#         #=> #<MatchData "ototo" vowel:"o">
#
# *Note*: A regexp can't use named backreferences and numbered
# backreferences simultaneously. Also, if a named capture is used in a
# regexp, then parentheses used for grouping which would otherwise result
# in a unnamed capture are treated as non-capturing.
#
#     /(\w)(\w)/.match("ab").captures # => ["a", "b"]
#     /(\w)(\w)/.match("ab").named_captures # => {}
#
#     /(?<c>\w)(\w)/.match("ab").captures # => ["a"]
#     /(?<c>\w)(\w)/.match("ab").named_captures # => {"c"=>"a"}
#
# When named capture groups are used with a literal regexp on the left-hand
# side of an expression and the <tt>=~</tt> operator, the captured text is
# also assigned to local variables with corresponding names.
#
#     /\$(?<dollars>\d+)\.(?<cents>\d+)/ =~ "$3.67" #=> 0
#     dollars #=> "3"
#
# == Grouping
#
# Parentheses also <i>group</i> the terms they enclose, allowing them to be
# quantified as one <i>atomic</i> whole.
#
# The pattern below matches a vowel followed by 2 word characters:
#
#     /[aeiou]\w{2}/.match("Caenorhabditis elegans") #=> #<MatchData "aen">
#
# Whereas the following pattern matches a vowel followed by a word character,
# twice, i.e. <tt>[aeiou]\w[aeiou]\w</tt>: 'enor'.
#
#     /([aeiou]\w){2}/.match("Caenorhabditis elegans")
#         #=> #<MatchData "enor" 1:"or">
#
# The <tt>(?:</tt>...<tt>)</tt> construct provides grouping without
# capturing. That is, it combines the terms it contains into an atomic whole
# without creating a backreference. This benefits performance at the slight
# expense of readability.
#
# The first group of parentheses captures 'n' and the second 'ti'. The second
# group is referred to later with the backreference <tt>\2</tt>:
#
#     /I(n)ves(ti)ga\2ons/.match("Investigations")
#         #=> #<MatchData "Investigations" 1:"n" 2:"ti">
#
# The first group of parentheses is now made non-capturing with '?:', so it
# still matches 'n', but doesn't create the backreference. Thus, the
# backreference <tt>\1</tt> now refers to 'ti'.
#
#     /I(?:n)ves(ti)ga\1ons/.match("Investigations")
#         #=> #<MatchData "Investigations" 1:"ti">
#
# === Atomic Grouping
#
# Grouping can be made <i>atomic</i> with
# <tt>(?></tt><i>pat</i><tt>)</tt>. This causes the subexpression <i>pat</i>
# to be matched independently of the rest of the expression such that what
# it matches becomes fixed for the remainder of the match, unless the entire
# subexpression must be abandoned and subsequently revisited. In this
# way <i>pat</i> is treated as a non-divisible whole. Atomic grouping is
# typically used to optimise patterns so as to prevent the regular
# expression engine from backtracking needlessly.
#
# The <tt>"</tt> in the pattern below matches the first character of the string,
# then <tt>.*</tt> matches <i>Quote"</i>. This causes the overall match to fail,
# so the text matched by <tt>.*</tt> is backtracked by one position, which
# leaves the final character of the string available to match <tt>"</tt>
#
#           /".*"/.match('"Quote"')     #=> #<MatchData "\"Quote\"">
#
# If <tt>.*</tt> is grouped atomically, it refuses to backtrack <i>Quote"</i>,
# even though this means that the overall match fails
#
#     /"(?>.*)"/.match('"Quote"') #=> nil
#
# == Subexpression Calls
#
# The <tt>\g<</tt><i>name</i><tt>></tt> syntax matches the previous
# subexpression named _name_, which can be a group name or number, again.
# This differs from backreferences in that it re-executes the group rather
# than simply trying to re-match the same text.
#
# This pattern matches a <i>(</i> character and assigns it to the <tt>paren</tt>
# group, tries to call that the <tt>paren</tt> sub-expression again but fails,
# then matches a literal <i>)</i>:
#
#     /\A(?<paren>\(\g<paren>*\))*\z/ =~ '()'
#
#
#     /\A(?<paren>\(\g<paren>*\))*\z/ =~ '(())' #=> 0
#     # ^1
#     #      ^2
#     #           ^3
#     #                 ^4
#     #      ^5
#     #           ^6
#     #                      ^7
#     #                       ^8
#     #                       ^9
#     #                           ^10
#
# 1.  Matches at the beginning of the string, i.e. before the first
#     character.
# 2.  Enters a named capture group called <tt>paren</tt>
# 3.  Matches a literal <i>(</i>, the first character in the string
# 4.  Calls the <tt>paren</tt> group again, i.e. recurses back to the
#     second step
# 5.  Re-enters the <tt>paren</tt> group
# 6.  Matches a literal <i>(</i>, the second character in the
#     string
# 7.  Try to call <tt>paren</tt> a third time, but fail because
#     doing so would prevent an overall successful match
# 8.  Match a literal <i>)</i>, the third character in the string.
#     Marks the end of the second recursive call
# 9.  Match a literal <i>)</i>, the fourth character in the string
# 10. Match the end of the string
#
# == Alternation
#
# The vertical bar metacharacter (<tt>|</tt>) combines several expressions into
# a single one that matches any of the expressions. Each expression is an
# <i>alternative</i>.
#
#     /\w(and|or)\w/.match("Feliformia") #=> #<MatchData "form" 1:"or">
#     /\w(and|or)\w/.match("furandi")    #=> #<MatchData "randi" 1:"and">
#     /\w(and|or)\w/.match("dissemblance") #=> nil
#
# == Character Properties
#
# The <tt>\p{}</tt> construct matches characters with the named property,
# much like POSIX bracket classes.
#
# * <tt>/\p{Alnum}/</tt> - Alphabetic and numeric character
# * <tt>/\p{Alpha}/</tt> - Alphabetic character
# * <tt>/\p{Blank}/</tt> - Space or tab
# * <tt>/\p{Cntrl}/</tt> - Control character
# * <tt>/\p{Digit}/</tt> - Digit
# * <tt>/\p{Emoji}/</tt> - Unicode emoji
# * <tt>/\p{Graph}/</tt> - Non-blank character (excludes spaces, control
#   characters, and similar)
# * <tt>/\p{Lower}/</tt> - Lowercase alphabetical character
# * <tt>/\p{Print}/</tt> - Like <tt>\p{Graph}</tt>, but includes the space character
# * <tt>/\p{Punct}/</tt> - Punctuation character
# * <tt>/\p{Space}/</tt> - Whitespace character (<tt>[:blank:]</tt>, newline,
#   carriage return, etc.)
# * <tt>/\p{Upper}/</tt> - Uppercase alphabetical
# * <tt>/\p{XDigit}/</tt> - Digit allowed in a hexadecimal number (i.e., 0-9a-fA-F)
# * <tt>/\p{Word}/</tt> - A member of one of the following Unicode general
#   category <i>Letter</i>, <i>Mark</i>, <i>Number</i>,
#   <i>Connector\_Punctuation</i>
# * <tt>/\p{ASCII}/</tt> - A character in the ASCII character set
# * <tt>/\p{Any}/</tt> - Any Unicode character (including unassigned
#   characters)
# * <tt>/\p{Assigned}/</tt> - An assigned character
#
# A Unicode character's <i>General Category</i> value can also be matched
# with <tt>\p{</tt><i>Ab</i><tt>}</tt> where <i>Ab</i> is the category's
# abbreviation as described below:
#
# * <tt>/\p{L}/</tt> - 'Letter'
# * <tt>/\p{Ll}/</tt> - 'Letter: Lowercase'
# * <tt>/\p{Lm}/</tt> - 'Letter: Mark'
# * <tt>/\p{Lo}/</tt> - 'Letter: Other'
# * <tt>/\p{Lt}/</tt> - 'Letter: Titlecase'
# * <tt>/\p{Lu}/</tt> - 'Letter: Uppercase
# * <tt>/\p{Lo}/</tt> - 'Letter: Other'
# * <tt>/\p{M}/</tt> - 'Mark'
# * <tt>/\p{Mn}/</tt> - 'Mark: Nonspacing'
# * <tt>/\p{Mc}/</tt> - 'Mark: Spacing Combining'
# * <tt>/\p{Me}/</tt> - 'Mark: Enclosing'
# * <tt>/\p{N}/</tt> - 'Number'
# * <tt>/\p{Nd}/</tt> - 'Number: Decimal Digit'
# * <tt>/\p{Nl}/</tt> - 'Number: Letter'
# * <tt>/\p{No}/</tt> - 'Number: Other'
# * <tt>/\p{P}/</tt> - 'Punctuation'
# * <tt>/\p{Pc}/</tt> - 'Punctuation: Connector'
# * <tt>/\p{Pd}/</tt> - 'Punctuation: Dash'
# * <tt>/\p{Ps}/</tt> - 'Punctuation: Open'
# * <tt>/\p{Pe}/</tt> - 'Punctuation: Close'
# * <tt>/\p{Pi}/</tt> - 'Punctuation: Initial Quote'
# * <tt>/\p{Pf}/</tt> - 'Punctuation: Final Quote'
# * <tt>/\p{Po}/</tt> - 'Punctuation: Other'
# * <tt>/\p{S}/</tt> - 'Symbol'
# * <tt>/\p{Sm}/</tt> - 'Symbol: Math'
# * <tt>/\p{Sc}/</tt> - 'Symbol: Currency'
# * <tt>/\p{Sc}/</tt> - 'Symbol: Currency'
# * <tt>/\p{Sk}/</tt> - 'Symbol: Modifier'
# * <tt>/\p{So}/</tt> - 'Symbol: Other'
# * <tt>/\p{Z}/</tt> - 'Separator'
# * <tt>/\p{Zs}/</tt> - 'Separator: Space'
# * <tt>/\p{Zl}/</tt> - 'Separator: Line'
# * <tt>/\p{Zp}/</tt> - 'Separator: Paragraph'
# * <tt>/\p{C}/</tt> - 'Other'
# * <tt>/\p{Cc}/</tt> - 'Other: Control'
# * <tt>/\p{Cf}/</tt> - 'Other: Format'
# * <tt>/\p{Cn}/</tt> - 'Other: Not Assigned'
# * <tt>/\p{Co}/</tt> - 'Other: Private Use'
# * <tt>/\p{Cs}/</tt> - 'Other: Surrogate'
#
# Lastly, <tt>\p{}</tt> matches a character's Unicode <i>script</i>. The
# following scripts are supported: <i>Arabic</i>, <i>Armenian</i>,
# <i>Balinese</i>, <i>Bengali</i>, <i>Bopomofo</i>, <i>Braille</i>,
# <i>Buginese</i>, <i>Buhid</i>, <i>Canadian_Aboriginal</i>, <i>Carian</i>,
# <i>Cham</i>, <i>Cherokee</i>, <i>Common</i>, <i>Coptic</i>,
# <i>Cuneiform</i>, <i>Cypriot</i>, <i>Cyrillic</i>, <i>Deseret</i>,
# <i>Devanagari</i>, <i>Ethiopic</i>, <i>Georgian</i>, <i>Glagolitic</i>,
# <i>Gothic</i>, <i>Greek</i>, <i>Gujarati</i>, <i>Gurmukhi</i>, <i>Han</i>,
# <i>Hangul</i>, <i>Hanunoo</i>, <i>Hebrew</i>, <i>Hiragana</i>,
# <i>Inherited</i>, <i>Kannada</i>, <i>Katakana</i>, <i>Kayah_Li</i>,
# <i>Kharoshthi</i>, <i>Khmer</i>, <i>Lao</i>, <i>Latin</i>, <i>Lepcha</i>,
# <i>Limbu</i>, <i>Linear_B</i>, <i>Lycian</i>, <i>Lydian</i>,
# <i>Malayalam</i>, <i>Mongolian</i>, <i>Myanmar</i>, <i>New_Tai_Lue</i>,
# <i>Nko</i>, <i>Ogham</i>, <i>Ol_Chiki</i>, <i>Old_Italic</i>,
# <i>Old_Persian</i>, <i>Oriya</i>, <i>Osmanya</i>, <i>Phags_Pa</i>,
# <i>Phoenician</i>, <i>Rejang</i>, <i>Runic</i>, <i>Saurashtra</i>,
# <i>Shavian</i>, <i>Sinhala</i>, <i>Sundanese</i>, <i>Syloti_Nagri</i>,
# <i>Syriac</i>, <i>Tagalog</i>, <i>Tagbanwa</i>, <i>Tai_Le</i>,
# <i>Tamil</i>, <i>Telugu</i>, <i>Thaana</i>, <i>Thai</i>, <i>Tibetan</i>,
# <i>Tifinagh</i>, <i>Ugaritic</i>, <i>Vai</i>, and <i>Yi</i>.
#
# Unicode codepoint U+06E9 is named "ARABIC PLACE OF SAJDAH" and belongs to the
# Arabic script:
#
#     /\p{Arabic}/.match("\u06E9") #=> #<MatchData "\u06E9">
#
# All character properties can be inverted by prefixing their name with a
# caret (<tt>^</tt>).
#
# Letter 'A' is not in the Unicode Ll (Letter; Lowercase) category, so this
# match succeeds:
#
#     /\p{^Ll}/.match("A") #=> #<MatchData "A">
#
# == Anchors
#
# Anchors are metacharacter that match the zero-width positions between
# characters, <i>anchoring</i> the match to a specific position.
#
# * <tt>^</tt> - Matches beginning of line
# * <tt>$</tt> - Matches end of line
# * <tt>\A</tt> - Matches beginning of string.
# * <tt>\Z</tt> - Matches end of string. If string ends with a newline,
#   it matches just before newline
# * <tt>\z</tt> - Matches end of string
# * <tt>\G</tt> - Matches first matching position:
#
#   In methods like <tt>String#gsub</tt> and <tt>String#scan</tt>, it changes on each iteration.
#   It initially matches the beginning of subject, and in each following iteration it matches where the last match finished.
#
#       "    a b c".gsub(/ /, '_')    #=> "____a_b_c"
#       "    a b c".gsub(/\G /, '_')  #=> "____a b c"
#
#   In methods like <tt>Regexp#match</tt> and <tt>String#match</tt> that take an (optional) offset, it matches where the search begins.
#
#       "hello, world".match(/,/, 3)    #=> #<MatchData ",">
#       "hello, world".match(/\G,/, 3)  #=> nil
#
# * <tt>\b</tt> - Matches word boundaries when outside brackets;
#   backspace (0x08) when inside brackets
# * <tt>\B</tt> - Matches non-word boundaries
# * <tt>(?=</tt><i>pat</i><tt>)</tt> - <i>Positive lookahead</i> assertion:
#   ensures that the following characters match <i>pat</i>, but doesn't
#   include those characters in the matched text
# * <tt>(?!</tt><i>pat</i><tt>)</tt> - <i>Negative lookahead</i> assertion:
#   ensures that the following characters do not match <i>pat</i>, but
#   doesn't include those characters in the matched text
# * <tt>(?<=</tt><i>pat</i><tt>)</tt> - <i>Positive lookbehind</i>
#   assertion: ensures that the preceding characters match <i>pat</i>, but
#   doesn't include those characters in the matched text
# * <tt>(?<!</tt><i>pat</i><tt>)</tt> - <i>Negative lookbehind</i>
#   assertion: ensures that the preceding characters do not match
#   <i>pat</i>, but doesn't include those characters in the matched text
#
# * <tt>\K</tt> - <i>Match reset</i>: the matched content preceding
#   <tt>\K</tt> in the regexp is excluded from the result.  For example,
#   the following two regexps are almost equivalent:
#
#       /ab\Kc/ =~ "abc"     #=> 0
#       /(?<=ab)c/ =~ "abc"  #=> 2
#
#   These match same string and <i>$&</i> equals <tt>"c"</tt>, while the
#   matched position is different.
#
#   As are the following two regexps:
#
#       /(a)\K(b)\Kc/
#       /(?<=(?<=(a))(b))c/
#
# If a pattern isn't anchored it can begin at any point in the string:
#
#     /real/.match("surrealist") #=> #<MatchData "real">
#
# Anchoring the pattern to the beginning of the string forces the match to start
# there. 'real' doesn't occur at the beginning of the string, so now the match
# fails:
#
#     /\Areal/.match("surrealist") #=> nil
#
# The match below fails because although 'Demand' contains 'and', the pattern
# does not occur at a word boundary.
#
#     /\band/.match("Demand")
#
# Whereas in the following example 'and' has been anchored to a non-word
# boundary so instead of matching the first 'and' it matches from the fourth
# letter of 'demand' instead:
#
#     /\Band.+/.match("Supply and demand curve") #=> #<MatchData "and curve">
#
# The pattern below uses positive lookahead and positive lookbehind to match
# text appearing in <b></b> tags without including the tags in the match:
#
#     /(?<=<b>)\w+(?=<\/b>)/.match("Fortune favours the <b>bold</b>")
#         #=> #<MatchData "bold">
#
# == Options
#
# The end delimiter for a regexp can be followed by one or more single-letter
# options which control how the pattern can match.
#
# * <tt>/pat/i</tt> - Ignore case
# * <tt>/pat/m</tt> - Treat a newline as a character matched by <tt>.</tt>
# * <tt>/pat/x</tt> - Ignore whitespace and comments in the pattern
# * <tt>/pat/o</tt> - Perform <tt>#{}</tt> interpolation only once
#
# <tt>i</tt>, <tt>m</tt>, and <tt>x</tt> can also be applied on the
# subexpression level with the
# <tt>(?</tt><i>on</i><tt>-</tt><i>off</i><tt>)</tt> construct, which
# enables options <i>on</i>, and disables options <i>off</i> for the
# expression enclosed by the parentheses:
#
#     /a(?i:b)c/.match('aBc')   #=> #<MatchData "aBc">
#     /a(?-i:b)c/i.match('ABC') #=> nil
#
# Additionally, these options can also be toggled for the remainder of the
# pattern:
#
#     /a(?i)bc/.match('abC') #=> #<MatchData "abC">
#
# Options may also be used with <tt>Regexp.new</tt>:
#
#     Regexp.new("abc", Regexp::IGNORECASE)                     #=> /abc/i
#     Regexp.new("abc", Regexp::MULTILINE)                      #=> /abc/m
#     Regexp.new("abc # Comment", Regexp::EXTENDED)             #=> /abc # Comment/x
#     Regexp.new("abc", Regexp::IGNORECASE | Regexp::MULTILINE) #=> /abc/mi
#
#     Regexp.new("abc", "i")           #=> /abc/i
#     Regexp.new("abc", "m")           #=> /abc/m
#     Regexp.new("abc # Comment", "x") #=> /abc # Comment/x
#     Regexp.new("abc", "im")          #=> /abc/mi
#
# == Free-Spacing Mode and Comments
#
# As mentioned above, the <tt>x</tt> option enables <i>free-spacing</i>
# mode. Literal white space inside the pattern is ignored, and the
# octothorpe (<tt>#</tt>) character introduces a comment until the end of
# the line. This allows the components of the pattern to be organized in a
# potentially more readable fashion.
#
# A contrived pattern to match a number with optional decimal places:
#
#     float_pat = /\A
#         [[:digit:]]+ # 1 or more digits before the decimal point
#         (\.          # Decimal point
#             [[:digit:]]+ # 1 or more digits after the decimal point
#         )? # The decimal point and following digits are optional
#     \Z/x
#     float_pat.match('3.14') #=> #<MatchData "3.14" 1:".14">
#
# There are a number of strategies for matching whitespace:
#
# * Use a pattern such as <tt>\s</tt> or <tt>\p{Space}</tt>.
# * Use escaped whitespace such as <tt>\ </tt>, i.e. a space preceded by a backslash.
# * Use a character class such as <tt>[ ]</tt>.
#
# Comments can be included in a non-<tt>x</tt> pattern with the
# <tt>(?#</tt><i>comment</i><tt>)</tt> construct, where <i>comment</i> is
# arbitrary text ignored by the regexp engine.
#
# Comments in regexp literals cannot include unescaped terminator
# characters.
#
# == Encoding
#
# Regular expressions are assumed to use the source encoding. This can be
# overridden with one of the following modifiers.
#
# * <tt>/</tt><i>pat</i><tt>/u</tt> - UTF-8
# * <tt>/</tt><i>pat</i><tt>/e</tt> - EUC-JP
# * <tt>/</tt><i>pat</i><tt>/s</tt> - Windows-31J
# * <tt>/</tt><i>pat</i><tt>/n</tt> - ASCII-8BIT
#
# A regexp can be matched against a string when they either share an
# encoding, or the regexp's encoding is _US-ASCII_ and the string's encoding
# is ASCII-compatible.
#
# If a match between incompatible encodings is attempted an
# <tt>Encoding::CompatibilityError</tt> exception is raised.
#
# The <tt>Regexp#fixed_encoding?</tt> predicate indicates whether the regexp
# has a <i>fixed</i> encoding, that is one incompatible with ASCII. A
# regexp's encoding can be explicitly fixed by supplying
# <tt>Regexp::FIXEDENCODING</tt> as the second argument of
# <tt>Regexp.new</tt>:
#
#     r = Regexp.new("a".force_encoding("iso-8859-1"),Regexp::FIXEDENCODING)
#     r =~ "a\u3042"
#        # raises Encoding::CompatibilityError: incompatible encoding regexp match
#        #         (ISO-8859-1 regexp with UTF-8 string)
#
# == \Regexp Global Variables
#
# Pattern matching sets some global variables :
#
# * <tt>$~</tt> is equivalent to Regexp.last_match;
# * <tt>$&</tt> contains the complete matched text;
# * <tt>$`</tt> contains string before match;
# * <tt>$'</tt> contains string after match;
# * <tt>$1</tt>, <tt>$2</tt> and so on contain text matching first, second, etc
#   capture group;
# * <tt>$+</tt> contains last capture group.
#
# Example:
#
#     m = /s(\w{2}).*(c)/.match('haystack') #=> #<MatchData "stac" 1:"ta" 2:"c">
#     $~                                    #=> #<MatchData "stac" 1:"ta" 2:"c">
#     Regexp.last_match                     #=> #<MatchData "stac" 1:"ta" 2:"c">
#
#     $&      #=> "stac"
#             # same as m[0]
#     $`      #=> "hay"
#             # same as m.pre_match
#     $'      #=> "k"
#             # same as m.post_match
#     $1      #=> "ta"
#             # same as m[1]
#     $2      #=> "c"
#             # same as m[2]
#     $3      #=> nil
#             # no third group in pattern
#     $+      #=> "c"
#             # same as m[-1]
#
# These global variables are thread-local and method-local variables.
#
# == Performance
#
# Certain pathological combinations of constructs can lead to abysmally bad
# performance.
#
# Consider a string of 25 <i>a</i>s, a <i>d</i>, 4 <i>a</i>s, and a
# <i>c</i>.
#
#     s = 'a' * 25 + 'd' + 'a' * 4 + 'c'
#     #=> "aaaaaaaaaaaaaaaaaaaaaaaaadaaaac"
#
# The following patterns match instantly as you would expect:
#
#     /(b|a)/ =~ s #=> 0
#     /(b|a+)/ =~ s #=> 0
#     /(b|a+)*/ =~ s #=> 0
#
# However, the following pattern takes appreciably longer:
#
#     /(b|a+)*c/ =~ s #=> 26
#
# This happens because an atom in the regexp is quantified by both an
# immediate <tt>+</tt> and an enclosing <tt>*</tt> with nothing to
# differentiate which is in control of any particular character. The
# nondeterminism that results produces super-linear performance. (Consult
# <i>Mastering Regular Expressions</i> (3rd ed.), pp 222, by
# <i>Jeffery Friedl</i>, for an in-depth analysis). This particular case
# can be fixed by use of atomic grouping, which prevents the unnecessary
# backtracking:
#
#     (start = Time.now) && /(b|a+)*c/ =~ s && (Time.now - start)
#        #=> 24.702736882
#     (start = Time.now) && /(?>b|a+)*c/ =~ s && (Time.now - start)
#        #=> 0.000166571
#
# A similar case is typified by the following example, which takes
# approximately 60 seconds to execute for me:
#
# Match a string of 29 <i>a</i>s against a pattern of 29 optional <i>a</i>s
# followed by 29 mandatory <i>a</i>s:
#
#     Regexp.new('a?' * 29 + 'a' * 29) =~ 'a' * 29
#
# The 29 optional <i>a</i>s match the string, but this prevents the 29
# mandatory <i>a</i>s that follow from matching. Ruby must then backtrack
# repeatedly so as to satisfy as many of the optional matches as it can
# while still matching the mandatory 29. It is plain to us that none of the
# optional matches can succeed, but this fact unfortunately eludes Ruby.
#
# The best way to improve performance is to significantly reduce the amount of
# backtracking needed.  For this case, instead of individually matching 29
# optional <i>a</i>s, a range of optional <i>a</i>s can be matched all at once
# with <i>a{0,29}</i>:
#
#     Regexp.new('a{0,29}' + 'a' * 29) =~ 'a' * 29
#
# == Timeout
#
# There are two APIs to set timeout. One is Regexp.timeout=, which is
# process-global configuration of timeout for Regexp matching.
#
#     Regexp.timeout = 3
#     s = 'a' * 25 + 'd' + 'a' * 4 + 'c'
#     /(b|a+)*c/ =~ s  #=> This raises an exception in three seconds
#
# The other is timeout keyword of Regexp.new.
#
#     re = Regexp.new("(b|a+)*c", timeout: 3)
#     s = 'a' * 25 + 'd' + 'a' * 4 + 'c'
#     /(b|a+)*c/ =~ s  #=> This raises an exception in three seconds
#
# When using Regexps to process untrusted input, you should use the timeout
# feature to avoid excessive backtracking. Otherwise, a malicious user can
# provide input to Regexp causing Denial-of-Service attack.
# Note that the timeout is not set by default because an appropriate limit
# highly depends on an application requirement and context.
class Regexp
  # see Regexp.options and Regexp.new
  EXTENDED = _
  # see Regexp.options and Regexp.new
  FIXEDENCODING = _
  # see Regexp.options and Regexp.new
  IGNORECASE = _
  # see Regexp.options and Regexp.new
  MULTILINE = _
  # see Regexp.options and Regexp.new
  NOENCODING = _

  # Alias for Regexp.new
  def self.compile(*args) end

  # Returns a new string that escapes any characters
  # that have special meaning in a regular expression:
  #
  #   s = Regexp.escape('\*?{}.')      # => "\\\\\\*\\?\\{\\}\\."
  #
  # For any string +s+, this call returns a MatchData object:
  #
  #   r = Regexp.new(Regexp.escape(s)) # => /\\\\\\\*\\\?\\\{\\\}\\\./
  #   r.match(s)                       # => #<MatchData "\\\\\\*\\?\\{\\}\\.">
  #
  # Regexp.quote is an alias for Regexp.escape.
  def self.escape(string) end

  # With no argument, returns the value of <tt>$!</tt>,
  # which is the result of the most recent pattern match
  # (see {Regexp Global Variables}[rdoc-ref:Regexp@Regexp+Global+Variables]):
  #
  #   /c(.)t/ =~ 'cat'  # => 0
  #   Regexp.last_match # => #<MatchData "cat" 1:"a">
  #   /a/ =~ 'foo'      # => nil
  #   Regexp.last_match # => nil
  #
  # With non-negative integer argument +n+, returns the _n_th field in the
  # matchdata, if any, or nil if none:
  #
  #   /c(.)t/ =~ 'cat'     # => 0
  #   Regexp.last_match(0) # => "cat"
  #   Regexp.last_match(1) # => "a"
  #   Regexp.last_match(2) # => nil
  #
  # With negative integer argument +n+, counts backwards from the last field:
  #
  #   Regexp.last_match(-1)       # => "a"
  #
  # With string or symbol argument +name+,
  # returns the string value for the named capture, if any:
  #
  #   /(?<lhs>\w+)\s*=\s*(?<rhs>\w+)/ =~ 'var = val'
  #   Regexp.last_match        # => #<MatchData "var = val" lhs:"var"rhs:"val">
  #   Regexp.last_match(:lhs)  # => "var"
  #   Regexp.last_match('rhs') # => "val"
  #   Regexp.last_match('foo') # Raises IndexError.
  def self.last_match(...) end

  # Returns +true+ if matching against <tt>re</tt> can be
  # done in linear time to the input string.
  #
  #   Regexp.linear_time?(/re/) # => true
  #
  # Note that this is a property of the ruby interpreter, not of the argument
  # regular expression.  Identical regexp can or cannot run in linear time
  # depending on your ruby binary.  Neither forward nor backward compatibility
  # is guaranteed about the return value of this method.  Our current algorithm
  # is (*1) but this is subject to change in the future.  Alternative
  # implementations can also behave differently.  They might always return
  # false for everything.
  #
  # (*1): https://doi.org/10.1109/SP40001.2021.00032
  def self.linear_time?(...) end

  # Returns a new string that escapes any characters
  # that have special meaning in a regular expression:
  #
  #   s = Regexp.escape('\*?{}.')      # => "\\\\\\*\\?\\{\\}\\."
  #
  # For any string +s+, this call returns a MatchData object:
  #
  #   r = Regexp.new(Regexp.escape(s)) # => /\\\\\\\*\\\?\\\{\\\}\\\./
  #   r.match(s)                       # => #<MatchData "\\\\\\*\\?\\{\\}\\.">
  #
  # Regexp.quote is an alias for Regexp.escape.
  def self.quote(p1) end

  # It returns the current default timeout interval for Regexp matching in second.
  # +nil+ means no default timeout configuration.
  def self.timeout; end

  # It sets the default timeout interval for Regexp matching in second.
  # +nil+ means no default timeout configuration.
  # This configuration is process-global. If you want to set timeout for
  # each Regexp, use +timeout+ keyword for <code>Regexp.new</code>.
  #
  #    Regexp.timeout = 1
  #    /^a*b?a*$/ =~ "a" * 100000 + "x" #=> regexp match timeout (RuntimeError)
  def self.timeout=(p1) end

  # Returns +object+ if it is a regexp:
  #
  #   Regexp.try_convert(/re/) # => /re/
  #
  # Otherwise if +object+ responds to <tt>:to_regexp</tt>,
  # calls <tt>object.to_regexp</tt> and returns the result.
  #
  # Returns +nil+ if +object+ does not respond to <tt>:to_regexp</tt>.
  #
  #   Regexp.try_convert('re') # => nil
  #
  # Raises an exception unless <tt>object.to_regexp</tt> returns a regexp.
  def self.try_convert(object) end

  # Returns a new regexp that is the union of the given patterns:
  #
  #   r = Regexp.union(%w[cat dog])      # => /cat|dog/
  #   r.match('cat')      # => #<MatchData "cat">
  #   r.match('dog')      # => #<MatchData "dog">
  #   r.match('cog')      # => nil
  #
  # For each pattern that is a string, <tt>Regexp.new(pattern)</tt> is used:
  #
  #   Regexp.union('penzance')             # => /penzance/
  #   Regexp.union('a+b*c')                # => /a\+b\*c/
  #   Regexp.union('skiing', 'sledding')   # => /skiing|sledding/
  #   Regexp.union(['skiing', 'sledding']) # => /skiing|sledding/
  #
  # For each pattern that is a regexp, it is used as is,
  # including its flags:
  #
  #   Regexp.union(/foo/i, /bar/m, /baz/x)
  #   # => /(?i-mx:foo)|(?m-ix:bar)|(?x-mi:baz)/
  #   Regexp.union([/foo/i, /bar/m, /baz/x])
  #   # => /(?i-mx:foo)|(?m-ix:bar)|(?x-mi:baz)/
  #
  # With no arguments, returns <tt>/(?!)/</tt>:
  #
  #   Regexp.union # => /(?!)/
  #
  # If any regexp pattern contains captures, the behavior is unspecified.
  def self.union(...) end

  # With argument +string+ given, returns a new regexp with the given string
  # and options:
  #
  #   r = Regexp.new('foo') # => /foo/
  #   r.source              # => "foo"
  #   r.options             # => 0
  #
  # Optional argument +options+ is one of the following:
  #
  # - A String of options:
  #
  #     Regexp.new('foo', 'i')  # => /foo/i
  #     Regexp.new('foo', 'im') # => /foo/im
  #
  # - The logical OR of one or more of the constants
  #   Regexp::EXTENDED, Regexp::IGNORECASE, Regexp::MULTILINE, and
  #   Regexp::NOENCODING:
  #
  #     Regexp.new('foo', Regexp::IGNORECASE) # => /foo/i
  #     Regexp.new('foo', Regexp::EXTENDED)   # => /foo/x
  #     Regexp.new('foo', Regexp::MULTILINE)  # => /foo/m
  #     Regexp.new('foo', Regexp::NOENCODING)  # => /foo/n
  #     flags = Regexp::IGNORECASE | Regexp::EXTENDED |  Regexp::MULTILINE
  #     Regexp.new('foo', flags)              # => /foo/mix
  #
  # - +nil+ or +false+, which is ignored.
  #
  # If optional keyword argument +timeout+ is given,
  # its float value overrides the timeout interval for the class,
  # Regexp.timeout.
  # If +nil+ is passed as +timeout, it uses the timeout interval
  # for the class, Regexp.timeout.
  #
  # With argument +regexp+ given, returns a new regexp. The source,
  # options, timeout are the same as +regexp+. +options+ and +n_flag+
  # arguments are ineffective.  The timeout can be overridden by
  # +timeout+ keyword.
  #
  #     options = Regexp::MULTILINE
  #     r = Regexp.new('foo', options, timeout: 1.1) # => /foo/m
  #     r2 = Regexp.new(r)                           # => /foo/m
  #     r2.timeout                                   # => 1.1
  #     r3 = Regexp.new(r, timeout: 3.14)            # => /foo/m
  #     r3.timeout                                   # => 3.14
  #
  # Regexp.compile is an alias for Regexp.new.
  def initialize(...) end

  # Returns +true+ if +self+ finds a match in +string+:
  #
  #   /^[a-z]*$/ === 'HELLO' # => false
  #   /^[A-Z]*$/ === 'HELLO' # => true
  #
  # This method is called in case statements:
  #
  #   s = 'HELLO'
  #   case s
  #   when /\A[a-z]*\z/; print "Lower case\n"
  #   when /\A[A-Z]*\z/; print "Upper case\n"
  #   else               print "Mixed case\n"
  #   end # => "Upper case"
  def ===(string) end

  # Returns the integer index (in characters) of the first match
  # for +self+ and +string+, or +nil+ if none;
  # also sets the
  # {rdoc-ref:Regexp Global Variables}[rdoc-ref:Regexp@Regexp+Global+Variables]:
  #
  #   /at/ =~ 'input data' # => 7
  #   $~                   # => #<MatchData "at">
  #   /ax/ =~ 'input data' # => nil
  #   $~                   # => nil
  #
  # Assigns named captures to local variables of the same names
  # if and only if +self+:
  #
  # - Is a regexp literal;
  #   see {Regexp Literals}[rdoc-ref:literals.rdoc@Regexp+Literals].
  # - Does not contain interpolations;
  #   see {Regexp Interpolation}[rdoc-ref:Regexp@Regexp+Interpolation].
  # - Is at the left of the expression.
  #
  # Example:
  #
  #   /(?<lhs>\w+)\s*=\s*(?<rhs>\w+)/ =~ '  x = y  '
  #   p lhs # => "x"
  #   p rhs # => "y"
  #
  # Assigns +nil+ if not matched:
  #
  #   /(?<lhs>\w+)\s*=\s*(?<rhs>\w+)/ =~ '  x = '
  #   p lhs # => nil
  #   p rhs # => nil
  #
  # Does not make local variable assignments if +self+ is not a regexp literal:
  #
  #   r = /(?<foo>\w+)\s*=\s*(?<foo>\w+)/
  #   r =~ '  x = y  '
  #   p foo # Undefined local variable
  #   p bar # Undefined local variable
  #
  # The assignment does not occur if the regexp is not at the left:
  #
  #   '  x = y  ' =~ /(?<foo>\w+)\s*=\s*(?<foo>\w+)/
  #   p foo, foo # Undefined local variables
  #
  # A regexp interpolation, <tt>#{}</tt>, also disables
  # the assignment:
  #
  #   r = /(?<foo>\w+)/
  #   /(?<foo>\w+)\s*=\s*#{r}/ =~ 'x = y'
  #   p foo # Undefined local variable
  def =~(string) end

  # Equivalent to <tt><i>rxp</i> =~ $_</tt>:
  #
  #   $_ = "input data"
  #   ~ /at/ # => 7
  def ~; end

  # Returns +true+ if the case-insensitivity flag in +self+ is set,
  # +false+ otherwise:
  #
  #   /a/.casefold?           # => false
  #   /a/i.casefold?          # => true
  #   /(?i:a)/.casefold?      # => false
  def casefold?; end

  # Returns the Encoding object that represents the encoding of obj.
  def encoding; end

  # Returns +true+ if +object+ is another \Regexp whose pattern,
  # flags, and encoding are the same as +self+, +false+ otherwise:
  #
  #   /foo/ == Regexp.new('foo')                          # => true
  #   /foo/ == /foo/i                                     # => false
  #   /foo/ == Regexp.new('food')                         # => false
  #   /foo/ == Regexp.new("abc".force_encoding("euc-jp")) # => false
  #
  # Regexp#eql? is an alias for Regexp#==.
  def eql?(other) end
  alias == eql?

  # Returns +false+ if +self+ is applicable to
  # a string with any ASCII-compatible encoding;
  # otherwise returns +true+:
  #
  #   r = /a/                                          # => /a/
  #   r.fixed_encoding?                               # => false
  #   r.match?("\u{6666} a")                          # => true
  #   r.match?("\xa1\xa2 a".force_encoding("euc-jp")) # => true
  #   r.match?("abc".force_encoding("euc-jp"))        # => true
  #
  #   r = /a/u                                        # => /a/
  #   r.fixed_encoding?                               # => true
  #   r.match?("\u{6666} a")                          # => true
  #   r.match?("\xa1\xa2".force_encoding("euc-jp"))   # Raises exception.
  #   r.match?("abc".force_encoding("euc-jp"))        # => true
  #
  #   r = /\u{6666}/                                  # => /\u{6666}/
  #   r.fixed_encoding?                               # => true
  #   r.encoding                                      # => #<Encoding:UTF-8>
  #   r.match?("\u{6666} a")                          # => true
  #   r.match?("\xa1\xa2".force_encoding("euc-jp"))   # Raises exception.
  #   r.match?("abc".force_encoding("euc-jp"))        # => false
  def fixed_encoding?; end

  # Returns the integer hash value for +self+.
  #
  # Related: Object#hash.
  def hash; end

  # Returns a nicely-formatted string representation of +self+:
  #
  #   /ab+c/ix.inspect # => "/ab+c/ix"
  #
  # Related: Regexp#to_s.
  def inspect; end

  # With no block given, returns the MatchData object
  # that describes the match, if any, or +nil+ if none;
  # the search begins at the given character +offset+ in +string+:
  #
  #   /abra/.match('abracadabra')      # => #<MatchData "abra">
  #   /abra/.match('abracadabra', 4)   # => #<MatchData "abra">
  #   /abra/.match('abracadabra', 8)   # => nil
  #   /abra/.match('abracadabra', 800) # => nil
  #
  #   string = "\u{5d0 5d1 5e8 5d0}cadabra"
  #   /abra/.match(string, 7)          #=> #<MatchData "abra">
  #   /abra/.match(string, 8)          #=> nil
  #   /abra/.match(string.b, 8)        #=> #<MatchData "abra">
  #
  # With a block given, calls the block if and only if a match is found;
  # returns the block's value:
  #
  #   /abra/.match('abracadabra') {|matchdata| p matchdata }
  #   # => #<MatchData "abra">
  #   /abra/.match('abracadabra', 4) {|matchdata| p matchdata }
  #   # => #<MatchData "abra">
  #   /abra/.match('abracadabra', 8) {|matchdata| p matchdata }
  #   # => nil
  #   /abra/.match('abracadabra', 8) {|marchdata| fail 'Cannot happen' }
  #   # => nil
  #
  # Output (from the first two blocks above):
  #
  #   #<MatchData "abra">
  #   #<MatchData "abra">
  #
  #    /(.)(.)(.)/.match("abc")[2] # => "b"
  #    /(.)(.)/.match("abc", 1)[2] # => "c"
  def match(string, offset = 0) end

  # Returns <code>true</code> or <code>false</code> to indicate whether the
  # regexp is matched or not without updating $~ and other related variables.
  # If the second parameter is present, it specifies the position in the string
  # to begin the search.
  #
  #    /R.../.match?("Ruby")    # => true
  #    /R.../.match?("Ruby", 1) # => false
  #    /P.../.match?("Ruby")    # => false
  #    $&                       # => nil
  def match?(...) end

  # Returns a hash representing named captures of +self+
  # (see {Named Captures}[rdoc-ref:Regexp@Named+Captures]):
  #
  # - Each key is the name of a named capture.
  # - Each value is an array of integer indexes for that named capture.
  #
  # Examples:
  #
  #   /(?<foo>.)(?<bar>.)/.named_captures # => {"foo"=>[1], "bar"=>[2]}
  #   /(?<foo>.)(?<foo>.)/.named_captures # => {"foo"=>[1, 2]}
  #   /(.)(.)/.named_captures             # => {}
  def named_captures; end

  # Returns an array of names of captures
  # (see {Named Captures}[rdoc-ref:Regexp@Named+Captures]):
  #
  #   /(?<foo>.)(?<bar>.)(?<baz>.)/.names # => ["foo", "bar", "baz"]
  #   /(?<foo>.)(?<foo>.)/.names          # => ["foo"]
  #   /(.)(.)/.names                      # => []
  def names; end

  # Returns an integer whose bits show the options set in +self+.
  #
  # The option bits are:
  #
  #   Regexp::IGNORECASE # => 1
  #   Regexp::EXTENDED   # => 2
  #   Regexp::MULTILINE  # => 4
  #
  # Examples:
  #
  #   /foo/.options    # => 0
  #   /foo/i.options   # => 1
  #   /foo/x.options   # => 2
  #   /foo/m.options   # => 4
  #   /foo/mix.options # => 7
  #
  # Note that additional bits may be set in the returned integer;
  # these are maintained internally internally in +self+,
  # are ignored if passed to Regexp.new, and may be ignored by the caller:
  #
  # Returns the set of bits corresponding to the options used when
  # creating this regexp (see Regexp::new for details). Note that
  # additional bits may be set in the returned options: these are used
  # internally by the regular expression code. These extra bits are
  # ignored if the options are passed to Regexp::new:
  #
  #   r = /\xa1\xa2/e                 # => /\xa1\xa2/
  #   r.source                        # => "\\xa1\\xa2"
  #   r.options                       # => 16
  #   Regexp.new(r.source, r.options) # => /\xa1\xa2/
  def options; end

  # Returns the original string of +self+:
  #
  #   /ab+c/ix.source # => "ab+c"
  #
  # Regexp escape sequences are retained:
  #
  #   /\x20\+/.source  # => "\\x20\\+"
  #
  # Lexer escape characters are not retained:
  #
  #   /\//.source  # => "/"
  def source; end

  # It returns the timeout interval for Regexp matching in second.
  # +nil+ means no default timeout configuration.
  #
  # This configuration is per-object. The global configuration set by
  # Regexp.timeout= is ignored if per-object configuration is set.
  #
  #    re = Regexp.new("^a*b?a*$", timeout: 1)
  #    re.timeout               #=> 1.0
  #    re =~ "a" * 100000 + "x" #=> regexp match timeout (RuntimeError)
  def timeout; end

  # Returns a string showing the options and string of +self+:
  #
  #   r0 = /ab+c/ix
  #   s0 = r0.to_s # => "(?ix-m:ab+c)"
  #
  # The returned string may be used as an argument to Regexp.new,
  # or as interpolated text for a
  # {Regexp literal}[rdoc-ref:regexp.rdoc@Regexp+Literal]:
  #
  #   r1 = Regexp.new(s0) # => /(?ix-m:ab+c)/
  #   r2 = /#{s0}/        # => /(?ix-m:ab+c)/
  #
  # Note that +r1+ and +r2+ are not equal to +r0+
  # because their original strings are different:
  #
  #   r0 == r1  # => false
  #   r0.source # => "ab+c"
  #   r1.source # => "(?ix-m:ab+c)"
  #
  # Related: Regexp#inspect.
  def to_s; end

  class TimeoutError < RegexpError
  end
end
