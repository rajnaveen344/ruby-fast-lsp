# frozen_string_literal: true

# A <code>String</code> object holds and manipulates an arbitrary sequence of
# bytes, typically representing characters. String objects may be created
# using <code>String::new</code> or as literals.
#
# Because of aliasing issues, users of strings should be aware of the methods
# that modify the contents of a <code>String</code> object.  Typically,
# methods with names ending in ``!'' modify their receiver, while those
# without a ``!'' return a new <code>String</code>.  However, there are
# exceptions, such as <code>String#[]=</code>.
class String
  include Comparable

  # Try to convert <i>obj</i> into a String, using to_str method.
  # Returns converted string or nil if <i>obj</i> cannot be converted
  # for any reason.
  #
  #    String.try_convert("str")     #=> "str"
  #    String.try_convert(/re/)      #=> nil
  def self.try_convert(obj) end

  # Returns a new string object containing a copy of <i>str</i>.
  #
  # The optional <i>encoding</i> keyword argument specifies the encoding
  # of the new string.
  # If not specified, the encoding of <i>str</i> is used
  # (or ASCII-8BIT, if <i>str</i> is not specified).
  #
  # The optional <i>capacity</i> keyword argument specifies the size
  # of the internal buffer.
  # This may improve performance, when the string will be concatenated many
  # times (causing many realloc calls).
  def initialize(*several_variants) end

  # Format---Uses <i>str</i> as a format specification, and returns the result
  # of applying it to <i>arg</i>. If the format specification contains more than
  # one substitution, then <i>arg</i> must be an <code>Array</code> or <code>Hash</code>
  # containing the values to be substituted. See <code>Kernel::sprintf</code> for
  # details of the format string.
  #
  #    "%05d" % 123                              #=> "00123"
  #    "%-5s: %016x" % [ "ID", self.object_id ]  #=> "ID   : 00002b054ec93168"
  #    "foo = %{foo}" % { :foo => 'bar' }        #=> "foo = bar"
  def %(other) end

  # Copy --- Returns a new String containing +integer+ copies of the receiver.
  # +integer+ must be greater than or equal to 0.
  #
  #    "Ho! " * 3   #=> "Ho! Ho! Ho! "
  #    "Ho! " * 0   #=> ""
  def *(other) end

  # Concatenation---Returns a new <code>String</code> containing
  # <i>other_str</i> concatenated to <i>str</i>.
  #
  #    "Hello from " + self.to_s   #=> "Hello from main"
  def +(other) end

  # If the string is frozen, then return duplicated mutable string.
  #
  # If the string is not frozen, then return the string itself.
  def +@; end

  # Returns a frozen, possibly pre-existing copy of the string.
  #
  # The string will be deduplicated as long as it is not tainted,
  # or has any instance variables set on it.
  def -@; end

  # Appends the given object to <i>str</i>. If the object is an
  # <code>Integer</code>, it is considered a codepoint and converted
  # to a character before being appended.
  #
  #    a = "hello "
  #    a << "world"   #=> "hello world"
  #    a << 33        #=> "hello world!"
  #
  # See also String#concat, which takes multiple arguments.
  def <<(*several_variants) end

  # Comparison---Returns -1, 0, +1, or +nil+ depending on whether +string+ is
  # less than, equal to, or greater than +other_string+.
  #
  # +nil+ is returned if the two values are incomparable.
  #
  # If the strings are of different lengths, and the strings are equal when
  # compared up to the shortest length, then the longer string is considered
  # greater than the shorter one.
  #
  # <code><=></code> is the basis for the methods <code><</code>,
  # <code><=</code>, <code>></code>, <code>>=</code>, and
  # <code>between?</code>, included from module Comparable. The method
  # String#== does not use Comparable#==.
  #
  #    "abcdef" <=> "abcde"     #=> 1
  #    "abcdef" <=> "abcdef"    #=> 0
  #    "abcdef" <=> "abcdefg"   #=> -1
  #    "abcdef" <=> "ABCDEF"    #=> 1
  #    "abcdef" <=> 1           #=> nil
  def <=>(other) end

  # Equality---Returns whether +str+ == +obj+, similar to Object#==.
  #
  # If +obj+ is not an instance of String but responds to +to_str+, then the
  # two strings are compared using <code>obj.==</code>.
  #
  # Otherwise, returns similarly to String#eql?, comparing length and content.
  def ==(other) end
  alias === ==

  # Match---If <i>obj</i> is a <code>Regexp</code>, use it as a pattern to match
  # against <i>str</i>,and returns the position the match starts, or
  # <code>nil</code> if there is no match. Otherwise, invokes
  # <i>obj.=~</i>, passing <i>str</i> as an argument. The default
  # <code>=~</code> in <code>Object</code> returns <code>nil</code>.
  #
  # Note: <code>str =~ regexp</code> is not the same as
  # <code>regexp =~ str</code>. Strings captured from named capture groups
  # are assigned to local variables only in the second case.
  #
  #    "cat o' 9 tails" =~ /\d/   #=> 7
  #    "cat o' 9 tails" =~ 9      #=> nil
  def =~(obj) end

  # Element Reference --- If passed a single +index+, returns a substring of
  # one character at that index. If passed a +start+ index and a +length+,
  # returns a substring containing +length+ characters starting at the
  # +start+ index. If passed a +range+, its beginning and end are interpreted as
  # offsets delimiting the substring to be returned.
  #
  # In these three cases, if an index is negative, it is counted from the end
  # of the string.  For the +start+ and +range+ cases the starting index
  # is just before a character and an index matching the string's size.
  # Additionally, an empty string is returned when the starting index for a
  # character range is at the end of the string.
  #
  # Returns +nil+ if the initial index falls outside the string or the length
  # is negative.
  #
  # If a +Regexp+ is supplied, the matching portion of the string is
  # returned.  If a +capture+ follows the regular expression, which may be a
  # capture group index or name, follows the regular expression that component
  # of the MatchData is returned instead.
  #
  # If a +match_str+ is given, that string is returned if it occurs in
  # the string.
  #
  # Returns +nil+ if the regular expression does not match or the match string
  # cannot be found.
  #
  #    a = "hello there"
  #
  #    a[1]                   #=> "e"
  #    a[2, 3]                #=> "llo"
  #    a[2..3]                #=> "ll"
  #
  #    a[-3, 2]               #=> "er"
  #    a[7..-2]               #=> "her"
  #    a[-4..-2]              #=> "her"
  #    a[-2..-4]              #=> ""
  #
  #    a[11, 0]               #=> ""
  #    a[11]                  #=> nil
  #    a[12, 0]               #=> nil
  #    a[12..-1]              #=> nil
  #
  #    a[/[aeiou](.)\1/]      #=> "ell"
  #    a[/[aeiou](.)\1/, 0]   #=> "ell"
  #    a[/[aeiou](.)\1/, 1]   #=> "l"
  #    a[/[aeiou](.)\1/, 2]   #=> nil
  #
  #    a[/(?<vowel>[aeiou])(?<non_vowel>[^aeiou])/, "non_vowel"] #=> "l"
  #    a[/(?<vowel>[aeiou])(?<non_vowel>[^aeiou])/, "vowel"]     #=> "e"
  #
  #    a["lo"]                #=> "lo"
  #    a["bye"]               #=> nil
  def [](*several_variants) end
  alias slice []

  # Element Assignment---Replaces some or all of the content of <i>str</i>. The
  # portion of the string affected is determined using the same criteria as
  # <code>String#[]</code>. If the replacement string is not the same length as
  # the text it is replacing, the string will be adjusted accordingly. If the
  # regular expression or string is used as the index doesn't match a position
  # in the string, <code>IndexError</code> is raised. If the regular expression
  # form is used, the optional second <code>Integer</code> allows you to specify
  # which portion of the match to replace (effectively using the
  # <code>MatchData</code> indexing rules. The forms that take an
  # <code>Integer</code> will raise an <code>IndexError</code> if the value is
  # out of range; the <code>Range</code> form will raise a
  # <code>RangeError</code>, and the <code>Regexp</code> and <code>String</code>
  # will raise an <code>IndexError</code> on negative match.
  def []=(*several_variants) end

  # Returns true for a string which has only ASCII characters.
  #
  #   "abc".force_encoding("UTF-8").ascii_only?          #=> true
  #   "abc\u{6666}".force_encoding("UTF-8").ascii_only?  #=> false
  def ascii_only?; end

  # Returns a copied string whose encoding is ASCII-8BIT.
  def b; end

  # Returns an array of bytes in <i>str</i>.  This is a shorthand for
  # <code>str.each_byte.to_a</code>.
  #
  # If a block is given, which is a deprecated form, works the same as
  # <code>each_byte</code>.
  def bytes; end

  # Returns the length of +str+ in bytes.
  #
  #   "\x80\u3042".bytesize  #=> 4
  #   "hello".bytesize       #=> 5
  def bytesize; end

  # Byte Reference---If passed a single <code>Integer</code>, returns a
  # substring of one byte at that position. If passed two <code>Integer</code>
  # objects, returns a substring starting at the offset given by the first, and
  # a length given by the second. If given a <code>Range</code>, a substring containing
  # bytes at offsets given by the range is returned. In all three cases, if
  # an offset is negative, it is counted from the end of <i>str</i>. Returns
  # <code>nil</code> if the initial offset falls outside the string, the length
  # is negative, or the beginning of the range is greater than the end.
  # The encoding of the resulted string keeps original encoding.
  #
  #    "hello".byteslice(1)     #=> "e"
  #    "hello".byteslice(-1)    #=> "o"
  #    "hello".byteslice(1, 2)  #=> "el"
  #    "\x80\u3042".byteslice(1, 3) #=> "\u3042"
  #    "\x03\u3042\xff".byteslice(1..3) #=> "\u3042"
  def byteslice(*several_variants) end

  # Returns a copy of <i>str</i> with the first character converted to uppercase
  # and the remainder to lowercase.
  #
  # See String#downcase for meaning of +options+ and use with different encodings.
  #
  #    "hello".capitalize    #=> "Hello"
  #    "HELLO".capitalize    #=> "Hello"
  #    "123ABC".capitalize   #=> "123abc"
  def capitalize(*several_variants) end

  # Modifies <i>str</i> by converting the first character to uppercase and the
  # remainder to lowercase. Returns <code>nil</code> if no changes are made.
  # There is an exception for modern Georgian (mkhedruli/MTAVRULI), where
  # the result is the same as for String#downcase, to avoid mixed case.
  #
  # See String#downcase for meaning of +options+ and use with different encodings.
  #
  #    a = "hello"
  #    a.capitalize!   #=> "Hello"
  #    a               #=> "Hello"
  #    a.capitalize!   #=> nil
  def capitalize!(*several_variants) end

  # Case-insensitive version of <code>String#<=></code>.
  # Currently, case-insensitivity only works on characters A-Z/a-z,
  # not all of Unicode. This is different from String#casecmp?.
  #
  #    "aBcDeF".casecmp("abcde")     #=> 1
  #    "aBcDeF".casecmp("abcdef")    #=> 0
  #    "aBcDeF".casecmp("abcdefg")   #=> -1
  #    "abcdef".casecmp("ABCDEF")    #=> 0
  #
  # +nil+ is returned if the two strings have incompatible encodings,
  # or if +other_str+ is not a string.
  #
  #    "foo".casecmp(2)   #=> nil
  #    "\u{e4 f6 fc}".encode("ISO-8859-1").casecmp("\u{c4 d6 dc}")   #=> nil
  def casecmp(other_str) end

  # Returns +true+ if +str+ and +other_str+ are equal after
  # Unicode case folding, +false+ if they are not equal.
  #
  #    "aBcDeF".casecmp?("abcde")     #=> false
  #    "aBcDeF".casecmp?("abcdef")    #=> true
  #    "aBcDeF".casecmp?("abcdefg")   #=> false
  #    "abcdef".casecmp?("ABCDEF")    #=> true
  #    "\u{e4 f6 fc}".casecmp?("\u{c4 d6 dc}")   #=> true
  #
  # +nil+ is returned if the two strings have incompatible encodings,
  # or if +other_str+ is not a string.
  #
  #    "foo".casecmp?(2)   #=> nil
  #    "\u{e4 f6 fc}".encode("ISO-8859-1").casecmp?("\u{c4 d6 dc}")   #=> nil
  def casecmp?(other_str) end

  # Centers +str+ in +width+.  If +width+ is greater than the length of +str+,
  # returns a new String of length +width+ with +str+ centered and padded with
  # +padstr+; otherwise, returns +str+.
  #
  #    "hello".center(4)         #=> "hello"
  #    "hello".center(20)        #=> "       hello        "
  #    "hello".center(20, '123') #=> "1231231hello12312312"
  def center(width, padstr = ' ') end

  # Returns an array of characters in <i>str</i>.  This is a shorthand
  # for <code>str.each_char.to_a</code>.
  #
  # If a block is given, which is a deprecated form, works the same as
  # <code>each_char</code>.
  def chars; end

  # Returns a new <code>String</code> with the given record separator removed
  # from the end of <i>str</i> (if present). If <code>$/</code> has not been
  # changed from the default Ruby record separator, then <code>chomp</code> also
  # removes carriage return characters (that is it will remove <code>\n</code>,
  # <code>\r</code>, and <code>\r\n</code>). If <code>$/</code> is an empty string,
  # it will remove all trailing newlines from the string.
  #
  #    "hello".chomp                #=> "hello"
  #    "hello\n".chomp              #=> "hello"
  #    "hello\r\n".chomp            #=> "hello"
  #    "hello\n\r".chomp            #=> "hello\n"
  #    "hello\r".chomp              #=> "hello"
  #    "hello \n there".chomp       #=> "hello \n there"
  #    "hello".chomp("llo")         #=> "he"
  #    "hello\r\n\r\n".chomp('')    #=> "hello"
  #    "hello\r\n\r\r\n".chomp('')  #=> "hello\r\n\r"
  def chomp(separator = $/) end

  # Modifies <i>str</i> in place as described for <code>String#chomp</code>,
  # returning <i>str</i>, or <code>nil</code> if no modifications were made.
  def chomp!(separator = $/) end

  # Returns a new <code>String</code> with the last character removed.  If the
  # string ends with <code>\r\n</code>, both characters are removed. Applying
  # <code>chop</code> to an empty string returns an empty
  # string. <code>String#chomp</code> is often a safer alternative, as it leaves
  # the string unchanged if it doesn't end in a record separator.
  #
  #    "string\r\n".chop   #=> "string"
  #    "string\n\r".chop   #=> "string\n"
  #    "string\n".chop     #=> "string"
  #    "string".chop       #=> "strin"
  #    "x".chop.chop       #=> ""
  def chop; end

  # Processes <i>str</i> as for <code>String#chop</code>, returning <i>str</i>,
  # or <code>nil</code> if <i>str</i> is the empty string.  See also
  # <code>String#chomp!</code>.
  def chop!; end

  # Returns a one-character string at the beginning of the string.
  #
  #    a = "abcde"
  #    a.chr    #=> "a"
  def chr; end

  # Makes string empty.
  #
  #    a = "abcde"
  #    a.clear    #=> ""
  def clear; end

  # Returns an array of the <code>Integer</code> ordinals of the
  # characters in <i>str</i>.  This is a shorthand for
  # <code>str.each_codepoint.to_a</code>.
  #
  # If a block is given, which is a deprecated form, works the same as
  # <code>each_codepoint</code>.
  def codepoints; end

  # Concatenates the given object(s) to <i>str</i>. If an object is an
  # <code>Integer</code>, it is considered a codepoint and converted
  # to a character before concatenation.
  #
  # +concat+ can take multiple arguments, and all the arguments are
  # concatenated in order.
  #
  #    a = "hello "
  #    a.concat("world", 33)      #=> "hello world!"
  #    a                          #=> "hello world!"
  #
  #    b = "sn"
  #    b.concat("_", b, "_", b)   #=> "sn_sn_sn"
  #
  # See also String#<<, which takes a single argument.
  def concat(*other_strings) end

  # Each +other_str+ parameter defines a set of characters to count.  The
  # intersection of these sets defines the characters to count in +str+.  Any
  # +other_str+ that starts with a caret <code>^</code> is negated.  The
  # sequence <code>c1-c2</code> means all characters between c1 and c2.  The
  # backslash character <code>\\</code> can be used to escape <code>^</code> or
  # <code>-</code> and is otherwise ignored unless it appears at the end of a
  # sequence or the end of a +other_str+.
  #
  #    a = "hello world"
  #    a.count "lo"                   #=> 5
  #    a.count "lo", "o"              #=> 2
  #    a.count "hello", "^l"          #=> 4
  #    a.count "ej-m"                 #=> 4
  #
  #    "hello^world".count "\\^aeiou" #=> 4
  #    "hello-world".count "a\\-eo"   #=> 4
  #
  #    c = "hello world\\r\\n"
  #    c.count "\\"                   #=> 2
  #    c.count "\\A"                  #=> 0
  #    c.count "X-\\w"                #=> 3
  def count(*args) end

  # Returns the string generated by calling <code>crypt(3)</code>
  # standard library function with <code>str</code> and
  # <code>salt_str</code>, in this order, as its arguments.  Please do
  # not use this method any longer.  It is legacy; provided only for
  # backward compatibility with ruby scripts in earlier days.  It is
  # bad to use in contemporary programs for several reasons:
  #
  #   * Behaviour of C's <code>crypt(3)</code> depends on the OS it is
  #     run.  The generated string lacks data portability.
  #
  #   * On some OSes such as Mac OS, <code>crypt(3)</code> never fails
  #     (i.e. silently ends up in unexpected results).
  #
  #   * On some OSes such as Mac OS, <code>crypt(3)</code> is not
  #     thread safe.
  #
  #   * So-called "traditional" usage of <code>crypt(3)</code> is very
  #     very very weak.  According to its manpage, Linux's traditional
  #     <code>crypt(3)</code> output has only 2**56 variations; too
  #     easy to brute force today.  And this is the default behaviour.
  #
  #   * In order to make things robust some OSes implement so-called
  #     "modular" usage. To go through, you have to do a complex
  #     build-up of the <code>salt_str</code> parameter, by hand.
  #     Failure in generation of a proper salt string tends not to
  #     yield any errors; typos in parameters are normally not
  #     detectable.
  #
  #       * For instance, in the following example, the second invocation
  #         of <code>String#crypt</code> is wrong; it has a typo in
  #         "round=" (lacks "s").  However the call does not fail and
  #         something unexpected is generated.
  #
  #            "foo".crypt("$5$rounds=1000$salt$") # OK, proper usage
  #            "foo".crypt("$5$round=1000$salt$")  # Typo not detected
  #
  #   * Even in the "modular" mode, some hash functions are considered
  #     archaic and no longer recommended at all; for instance module
  #     <code>$1$</code> is officially abandoned by its author: see
  #     http://phk.freebsd.dk/sagas/md5crypt_eol.html .  For another
  #     instance module <code>$3$</code> is considered completely
  #     broken: see the manpage of FreeBSD.
  #
  #   * On some OS such as Mac OS, there is no modular mode. Yet, as
  #     written above, <code>crypt(3)</code> on Mac OS never fails.
  #     This means even if you build up a proper salt string it
  #     generates a traditional DES hash anyways, and there is no way
  #     for you to be aware of.
  #
  #         "foo".crypt("$5$rounds=1000$salt$") # => "$5fNPQMxC5j6."
  #
  # If for some reason you cannot migrate to other secure contemporary
  # password hashing algorithms, install the string-crypt gem and
  # <code>require 'string/crypt'</code> to continue using it.
  def crypt(salt_str) end

  # Returns a copy of <i>str</i> with all characters in the intersection of its
  # arguments deleted. Uses the same rules for building the set of characters as
  # <code>String#count</code>.
  #
  #    "hello".delete "l","lo"        #=> "heo"
  #    "hello".delete "lo"            #=> "he"
  #    "hello".delete "aeiou", "^e"   #=> "hell"
  #    "hello".delete "ej-m"          #=> "ho"
  def delete(*other_strings) end

  # Performs a <code>delete</code> operation in place, returning <i>str</i>, or
  # <code>nil</code> if <i>str</i> was not modified.
  def delete!(*args) end

  # Returns a copy of <i>str</i> with leading <code>prefix</code> deleted.
  #
  #    "hello".delete_prefix("hel") #=> "lo"
  #    "hello".delete_prefix("llo") #=> "hello"
  def delete_prefix(prefix) end

  # Deletes leading <code>prefix</code> from <i>str</i>, returning
  # <code>nil</code> if no change was made.
  #
  #    "hello".delete_prefix!("hel") #=> "lo"
  #    "hello".delete_prefix!("llo") #=> nil
  def delete_prefix!(prefix) end

  # Returns a copy of <i>str</i> with trailing <code>suffix</code> deleted.
  #
  #    "hello".delete_suffix("llo") #=> "he"
  #    "hello".delete_suffix("hel") #=> "hello"
  def delete_suffix(suffix) end

  # Deletes trailing <code>suffix</code> from <i>str</i>, returning
  # <code>nil</code> if no change was made.
  #
  #    "hello".delete_suffix!("llo") #=> "he"
  #    "hello".delete_suffix!("hel") #=> nil
  def delete_suffix!(suffix) end

  # Returns a copy of <i>str</i> with all uppercase letters replaced with their
  # lowercase counterparts. Which letters exactly are replaced, and by which
  # other letters, depends on the presence or absence of options, and on the
  # +encoding+ of the string.
  #
  # The meaning of the +options+ is as follows:
  #
  # No option ::
  #   Full Unicode case mapping, suitable for most languages
  #   (see :turkic and :lithuanian options below for exceptions).
  #   Context-dependent case mapping as described in Table 3-14 of the
  #   Unicode standard is currently not supported.
  # :ascii ::
  #   Only the ASCII region, i.e. the characters ``A'' to ``Z'' and
  #   ``a'' to ``z'', are affected.
  #   This option cannot be combined with any other option.
  # :turkic ::
  #   Full Unicode case mapping, adapted for Turkic languages
  #   (Turkish, Azerbaijani, ...). This means that upper case I is mapped to
  #   lower case dotless i, and so on.
  # :lithuanian ::
  #   Currently, just full Unicode case mapping. In the future, full Unicode
  #   case mapping adapted for Lithuanian (keeping the dot on the lower case
  #   i even if there is an accent on top).
  # :fold ::
  #   Only available on +downcase+ and +downcase!+. Unicode case <b>folding</b>,
  #   which is more far-reaching than Unicode case mapping.
  #   This option currently cannot be combined with any other option
  #   (i.e. there is currently no variant for turkic languages).
  #
  # Please note that several assumptions that are valid for ASCII-only case
  # conversions do not hold for more general case conversions. For example,
  # the length of the result may not be the same as the length of the input
  # (neither in characters nor in bytes), some roundtrip assumptions
  # (e.g. str.downcase == str.upcase.downcase) may not apply, and Unicode
  # normalization (i.e. String#unicode_normalize) is not necessarily maintained
  # by case mapping operations.
  #
  # Non-ASCII case mapping/folding is currently supported for UTF-8,
  # UTF-16BE/LE, UTF-32BE/LE, and ISO-8859-1~16 Strings/Symbols.
  # This support will be extended to other encodings.
  #
  #    "hEllO".downcase   #=> "hello"
  def downcase(*several_variants) end

  # Downcases the contents of <i>str</i>, returning <code>nil</code> if no
  # changes were made.
  #
  # See String#downcase for meaning of +options+ and use with different encodings.
  def downcase!(*several_variants) end

  # Produces a version of +str+ with all non-printing characters replaced by
  # <code>\nnn</code> notation and all special characters escaped.
  #
  #   "hello \n ''".dump  #=> "\"hello \\n ''\""
  def dump; end

  # Passes each byte in <i>str</i> to the given block, or returns an
  # enumerator if no block is given.
  #
  #    "hello".each_byte {|c| print c, ' ' }
  #
  # <em>produces:</em>
  #
  #    104 101 108 108 111
  def each_byte; end

  # Passes each character in <i>str</i> to the given block, or returns
  # an enumerator if no block is given.
  #
  #    "hello".each_char {|c| print c, ' ' }
  #
  # <em>produces:</em>
  #
  #    h e l l o
  def each_char; end

  # Passes the <code>Integer</code> ordinal of each character in <i>str</i>,
  # also known as a <i>codepoint</i> when applied to Unicode strings to the
  # given block.  For encodings other than UTF-8/UTF-16(BE|LE)/UTF-32(BE|LE),
  # values are directly derived from the binary representation
  # of each character.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    "hello\u0639".each_codepoint {|c| print c, ' ' }
  #
  # <em>produces:</em>
  #
  #    104 101 108 108 111 1593
  def each_codepoint; end

  # Passes each grapheme cluster in <i>str</i> to the given block, or returns
  # an enumerator if no block is given.
  # Unlike String#each_char, this enumerates by grapheme clusters defined by
  # Unicode Standard Annex #29 http://unicode.org/reports/tr29/
  #
  #    "a\u0300".each_char.to_a.size #=> 2
  #    "a\u0300".each_grapheme_cluster.to_a.size #=> 1
  def each_grapheme_cluster; end

  # Splits <i>str</i> using the supplied parameter as the record
  # separator (<code>$/</code> by default), passing each substring in
  # turn to the supplied block.  If a zero-length record separator is
  # supplied, the string is split into paragraphs delimited by
  # multiple successive newlines.
  #
  # See IO.readlines for details about getline_args.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    print "Example one\n"
  #    "hello\nworld".each_line {|s| p s}
  #    print "Example two\n"
  #    "hello\nworld".each_line('l') {|s| p s}
  #    print "Example three\n"
  #    "hello\n\n\nworld".each_line('') {|s| p s}
  #
  # <em>produces:</em>
  #
  #    Example one
  #    "hello\n"
  #    "world"
  #    Example two
  #    "hel"
  #    "l"
  #    "o\nworl"
  #    "d"
  #    Example three
  #    "hello\n\n"
  #    "world"
  def each_line(*args) end

  # Returns <code>true</code> if <i>str</i> has a length of zero.
  #
  #    "hello".empty?   #=> false
  #    " ".empty?       #=> false
  #    "".empty?        #=> true
  def empty?; end

  # The first form returns a copy of +str+ transcoded
  # to encoding +encoding+.
  # The second form returns a copy of +str+ transcoded
  # from src_encoding to dst_encoding.
  # The last form returns a copy of +str+ transcoded to
  # <tt>Encoding.default_internal</tt>.
  #
  # By default, the first and second form raise
  # Encoding::UndefinedConversionError for characters that are
  # undefined in the destination encoding, and
  # Encoding::InvalidByteSequenceError for invalid byte sequences
  # in the source encoding. The last form by default does not raise
  # exceptions but uses replacement strings.
  #
  # The +options+ Hash gives details for conversion and can have the following
  # keys:
  #
  # :invalid ::
  #   If the value is +:replace+, #encode replaces invalid byte sequences in
  #   +str+ with the replacement character.  The default is to raise the
  #   Encoding::InvalidByteSequenceError exception
  # :undef ::
  #   If the value is +:replace+, #encode replaces characters which are
  #   undefined in the destination encoding with the replacement character.
  #   The default is to raise the Encoding::UndefinedConversionError.
  # :replace ::
  #   Sets the replacement string to the given value. The default replacement
  #   string is "\uFFFD" for Unicode encoding forms, and "?" otherwise.
  # :fallback ::
  #   Sets the replacement string by the given object for undefined
  #   character.  The object should be a Hash, a Proc, a Method, or an
  #   object which has [] method.
  #   Its key is an undefined character encoded in the source encoding
  #   of current transcoder. Its value can be any encoding until it
  #   can be converted into the destination encoding of the transcoder.
  # :xml ::
  #   The value must be +:text+ or +:attr+.
  #   If the value is +:text+ #encode replaces undefined characters with their
  #   (upper-case hexadecimal) numeric character references. '&', '<', and '>'
  #   are converted to "&amp;", "&lt;", and "&gt;", respectively.
  #   If the value is +:attr+, #encode also quotes the replacement result
  #   (using '"'), and replaces '"' with "&quot;".
  # :cr_newline ::
  #   Replaces LF ("\n") with CR ("\r") if value is true.
  # :crlf_newline ::
  #   Replaces LF ("\n") with CRLF ("\r\n") if value is true.
  # :universal_newline ::
  #   Replaces CRLF ("\r\n") and CR ("\r") with LF ("\n") if value is true.
  def encode(*several_variants) end

  # The first form transcodes the contents of <i>str</i> from
  # str.encoding to +encoding+.
  # The second form transcodes the contents of <i>str</i> from
  # src_encoding to dst_encoding.
  # The options Hash gives details for conversion. See String#encode
  # for details.
  # Returns the string even if no changes were made.
  def encode!(*several_variants) end

  # Returns the Encoding object that represents the encoding of obj.
  def encoding; end

  # Returns true if +str+ ends with one of the +suffixes+ given.
  #
  #   "hello".end_with?("ello")               #=> true
  #
  #   # returns true if one of the +suffixes+ matches.
  #   "hello".end_with?("heaven", "ello")     #=> true
  #   "hello".end_with?("heaven", "paradise") #=> false
  def end_with?(*args) end

  # Two strings are equal if they have the same length and content.
  def eql?(other) end

  # Changes the encoding to +encoding+ and returns self.
  def force_encoding(encoding) end

  def freeze; end

  # returns the <i>index</i>th byte as an integer.
  def getbyte(index) end

  # Returns an array of grapheme clusters in <i>str</i>.  This is a shorthand
  # for <code>str.each_grapheme_cluster.to_a</code>.
  #
  # If a block is given, which is a deprecated form, works the same as
  # <code>each_grapheme_cluster</code>.
  def grapheme_clusters; end

  # Returns a copy of <i>str</i> with <em>all</em> occurrences of
  # <i>pattern</i> substituted for the second argument. The <i>pattern</i> is
  # typically a <code>Regexp</code>; if given as a <code>String</code>, any
  # regular expression metacharacters it contains will be interpreted
  # literally, e.g. <code>'\\\d'</code> will match a backslash followed by 'd',
  # instead of a digit.
  #
  # If <i>replacement</i> is a <code>String</code> it will be substituted for
  # the matched text. It may contain back-references to the pattern's capture
  # groups of the form <code>\\\d</code>, where <i>d</i> is a group number, or
  # <code>\\\k<n></code>, where <i>n</i> is a group name. If it is a
  # double-quoted string, both back-references must be preceded by an
  # additional backslash. However, within <i>replacement</i> the special match
  # variables, such as <code>$&</code>, will not refer to the current match.
  #
  # If the second argument is a <code>Hash</code>, and the matched text is one
  # of its keys, the corresponding value is the replacement string.
  #
  # In the block form, the current match string is passed in as a parameter,
  # and variables such as <code>$1</code>, <code>$2</code>, <code>$`</code>,
  # <code>$&</code>, and <code>$'</code> will be set appropriately. The value
  # returned by the block will be substituted for the match on each call.
  #
  # The result inherits any tainting in the original string or any supplied
  # replacement string.
  #
  # When neither a block nor a second argument is supplied, an
  # <code>Enumerator</code> is returned.
  #
  #    "hello".gsub(/[aeiou]/, '*')                  #=> "h*ll*"
  #    "hello".gsub(/([aeiou])/, '<\1>')             #=> "h<e>ll<o>"
  #    "hello".gsub(/./) {|s| s.ord.to_s + ' '}      #=> "104 101 108 108 111 "
  #    "hello".gsub(/(?<foo>[aeiou])/, '{\k<foo>}')  #=> "h{e}ll{o}"
  #    'hello'.gsub(/[eo]/, 'e' => 3, 'o' => '*')    #=> "h3ll*"
  def gsub(*several_variants) end

  # Performs the substitutions of <code>String#gsub</code> in place, returning
  # <i>str</i>, or <code>nil</code> if no substitutions were performed.
  # If no block and no <i>replacement</i> is given, an enumerator is returned instead.
  def gsub!(*several_variants) end

  # Returns a hash based on the string's length, content and encoding.
  #
  # See also Object#hash.
  def hash; end

  # Treats leading characters from <i>str</i> as a string of hexadecimal digits
  # (with an optional sign and an optional <code>0x</code>) and returns the
  # corresponding number. Zero is returned on error.
  #
  #    "0x0a".hex     #=> 10
  #    "-1234".hex    #=> -4660
  #    "0".hex        #=> 0
  #    "wombat".hex   #=> 0
  def hex; end

  # Returns <code>true</code> if <i>str</i> contains the given string or
  # character.
  #
  #    "hello".include? "lo"   #=> true
  #    "hello".include? "ol"   #=> false
  #    "hello".include? ?h     #=> true
  def include?(other_str) end

  # Returns the index of the first occurrence of the given <i>substring</i> or
  # pattern (<i>regexp</i>) in <i>str</i>. Returns <code>nil</code> if not
  # found. If the second parameter is present, it specifies the position in the
  # string to begin the search.
  #
  #    "hello".index('e')             #=> 1
  #    "hello".index('lo')            #=> 3
  #    "hello".index('a')             #=> nil
  #    "hello".index(?e)              #=> 1
  #    "hello".index(/[aeiou]/, -3)   #=> 4
  def index(*several_variants) end

  # Replaces the contents and taintedness of <i>str</i> with the corresponding
  # values in <i>other_str</i>.
  #
  #    s = "hello"         #=> "hello"
  #    s.replace "world"   #=> "world"
  def initialize_copy(p1) end
  alias replace initialize_copy

  # Inserts <i>other_str</i> before the character at the given
  # <i>index</i>, modifying <i>str</i>. Negative indices count from the
  # end of the string, and insert <em>after</em> the given character.
  # The intent is insert <i>aString</i> so that it starts at the given
  # <i>index</i>.
  #
  #    "abcd".insert(0, 'X')    #=> "Xabcd"
  #    "abcd".insert(3, 'X')    #=> "abcXd"
  #    "abcd".insert(4, 'X')    #=> "abcdX"
  #    "abcd".insert(-3, 'X')   #=> "abXcd"
  #    "abcd".insert(-1, 'X')   #=> "abcdX"
  def insert(index, other_str) end

  # Returns a printable version of _str_, surrounded by quote marks,
  # with special characters escaped.
  #
  #    str = "hello"
  #    str[3] = "\b"
  #    str.inspect       #=> "\"hel\\bo\""
  def inspect; end

  # Returns the <code>Symbol</code> corresponding to <i>str</i>, creating the
  # symbol if it did not previously exist. See <code>Symbol#id2name</code>.
  #
  #    "Koala".intern         #=> :Koala
  #    s = 'cat'.to_sym       #=> :cat
  #    s == :cat              #=> true
  #    s = '@cat'.to_sym      #=> :@cat
  #    s == :@cat             #=> true
  #
  # This can also be used to create symbols that cannot be represented using the
  # <code>:xxx</code> notation.
  #
  #    'cat and dog'.to_sym   #=> :"cat and dog"
  def intern; end
  alias to_sym intern

  # Returns the character length of <i>str</i>.
  def length; end
  alias size length

  # Returns an array of lines in <i>str</i> split using the supplied
  # record separator (<code>$/</code> by default).  This is a
  # shorthand for <code>str.each_line(separator, getline_args).to_a</code>.
  #
  # See IO.readlines for details about getline_args.
  #
  #    "hello\nworld\n".lines              #=> ["hello\n", "world\n"]
  #    "hello  world".lines(' ')           #=> ["hello ", " ", "world"]
  #    "hello\nworld\n".lines(chomp: true) #=> ["hello", "world"]
  #
  # If a block is given, which is a deprecated form, works the same as
  # <code>each_line</code>.
  def lines(*args) end

  # If <i>integer</i> is greater than the length of <i>str</i>, returns a new
  # <code>String</code> of length <i>integer</i> with <i>str</i> left justified
  # and padded with <i>padstr</i>; otherwise, returns <i>str</i>.
  #
  #    "hello".ljust(4)            #=> "hello"
  #    "hello".ljust(20)           #=> "hello               "
  #    "hello".ljust(20, '1234')   #=> "hello123412341234123"
  def ljust(integer, padstr = ' ') end

  # Returns a copy of the receiver with leading whitespace removed.
  # See also String#rstrip and String#strip.
  #
  # Refer to String#strip for the definition of whitespace.
  #
  #    "  hello  ".lstrip   #=> "hello  "
  #    "hello".lstrip       #=> "hello"
  def lstrip; end

  # Removes leading whitespace from the receiver.
  # Returns the altered receiver, or +nil+ if no change was made.
  # See also String#rstrip! and String#strip!.
  #
  # Refer to String#strip for the definition of whitespace.
  #
  #    "  hello  ".lstrip!  #=> "hello  "
  #    "hello  ".lstrip!    #=> nil
  #    "hello".lstrip!      #=> nil
  def lstrip!; end

  # Converts <i>pattern</i> to a <code>Regexp</code> (if it isn't already one),
  # then invokes its <code>match</code> method on <i>str</i>.  If the second
  # parameter is present, it specifies the position in the string to begin the
  # search.
  #
  #    'hello'.match('(.)\1')      #=> #<MatchData "ll" 1:"l">
  #    'hello'.match('(.)\1')[0]   #=> "ll"
  #    'hello'.match(/(.)\1/)[0]   #=> "ll"
  #    'hello'.match(/(.)\1/, 3)   #=> nil
  #    'hello'.match('xx')         #=> nil
  #
  # If a block is given, invoke the block with MatchData if match succeed, so
  # that you can write
  #
  #    str.match(pat) {|m| ...}
  #
  # instead of
  #
  #    if m = str.match(pat)
  #      ...
  #    end
  #
  # The return value is a value from block execution in this case.
  def match(*several_variants) end

  # Converts _pattern_ to a +Regexp+ (if it isn't already one), then
  # returns a +true+ or +false+ indicates whether the regexp is
  # matched _str_ or not without updating <code>$~</code> and other
  # related variables.  If the second parameter is present, it
  # specifies the position in the string to begin the search.
  #
  #    "Ruby".match?(/R.../)    #=> true
  #    "Ruby".match?(/R.../, 1) #=> false
  #    "Ruby".match?(/P.../)    #=> false
  #    $&                       #=> nil
  def match?(*several_variants) end

  # Treats leading characters of <i>str</i> as a string of octal digits (with an
  # optional sign) and returns the corresponding number.  Returns 0 if the
  # conversion fails.
  #
  #    "123".oct       #=> 83
  #    "-377".oct      #=> -255
  #    "bad".oct       #=> 0
  #    "0377bad".oct   #=> 255
  #
  # If +str+ starts with <code>0</code>, radix indicators are honored.
  # See Kernel#Integer.
  def oct; end

  # Returns the <code>Integer</code> ordinal of a one-character string.
  #
  #    "a".ord         #=> 97
  def ord; end

  # Searches <i>sep</i> or pattern (<i>regexp</i>) in the string
  # and returns the part before it, the match, and the part
  # after it.
  # If it is not found, returns two empty strings and <i>str</i>.
  #
  #    "hello".partition("l")         #=> ["he", "l", "lo"]
  #    "hello".partition("x")         #=> ["hello", "", ""]
  #    "hello".partition(/.l/)        #=> ["h", "el", "lo"]
  def partition(*several_variants) end

  # Prepend---Prepend the given strings to <i>str</i>.
  #
  #    a = "!"
  #    a.prepend("hello ", "world") #=> "hello world!"
  #    a                            #=> "hello world!"
  #
  # See also String#concat.
  def prepend(*other_strings) end

  # Returns a new string with the characters from <i>str</i> in reverse order.
  #
  #    "stressed".reverse   #=> "desserts"
  def reverse; end

  # Reverses <i>str</i> in place.
  def reverse!; end

  # Returns the index of the last occurrence of the given <i>substring</i> or
  # pattern (<i>regexp</i>) in <i>str</i>. Returns <code>nil</code> if not
  # found. If the second parameter is present, it specifies the position in the
  # string to end the search---characters beyond this point will not be
  # considered.
  #
  #    "hello".rindex('e')             #=> 1
  #    "hello".rindex('l')             #=> 3
  #    "hello".rindex('a')             #=> nil
  #    "hello".rindex(?e)              #=> 1
  #    "hello".rindex(/[aeiou]/, -2)   #=> 1
  def rindex(*several_variants) end

  # If <i>integer</i> is greater than the length of <i>str</i>, returns a new
  # <code>String</code> of length <i>integer</i> with <i>str</i> right justified
  # and padded with <i>padstr</i>; otherwise, returns <i>str</i>.
  #
  #    "hello".rjust(4)            #=> "hello"
  #    "hello".rjust(20)           #=> "               hello"
  #    "hello".rjust(20, '1234')   #=> "123412341234123hello"
  def rjust(integer, padstr = ' ') end

  # Searches <i>sep</i> or pattern (<i>regexp</i>) in the string from the end
  # of the string, and returns the part before it, the match, and the part
  # after it.
  # If it is not found, returns two empty strings and <i>str</i>.
  #
  #    "hello".rpartition("l")         #=> ["hel", "l", "o"]
  #    "hello".rpartition("x")         #=> ["", "", "hello"]
  #    "hello".rpartition(/.l/)        #=> ["he", "ll", "o"]
  def rpartition(*several_variants) end

  # Returns a copy of the receiver with trailing whitespace removed.
  # See also String#lstrip and String#strip.
  #
  # Refer to String#strip for the definition of whitespace.
  #
  #    "  hello  ".rstrip   #=> "  hello"
  #    "hello".rstrip       #=> "hello"
  def rstrip; end

  # Removes trailing whitespace from the receiver.
  # Returns the altered receiver, or +nil+ if no change was made.
  # See also String#lstrip! and String#strip!.
  #
  # Refer to String#strip for the definition of whitespace.
  #
  #    "  hello  ".rstrip!  #=> "  hello"
  #    "  hello".rstrip!    #=> nil
  #    "hello".rstrip!      #=> nil
  def rstrip!; end

  # Both forms iterate through <i>str</i>, matching the pattern (which may be a
  # <code>Regexp</code> or a <code>String</code>). For each match, a result is
  # generated and either added to the result array or passed to the block. If
  # the pattern contains no groups, each individual result consists of the
  # matched string, <code>$&</code>.  If the pattern contains groups, each
  # individual result is itself an array containing one entry per group.
  #
  #    a = "cruel world"
  #    a.scan(/\w+/)        #=> ["cruel", "world"]
  #    a.scan(/.../)        #=> ["cru", "el ", "wor"]
  #    a.scan(/(...)/)      #=> [["cru"], ["el "], ["wor"]]
  #    a.scan(/(..)(..)/)   #=> [["cr", "ue"], ["l ", "wo"]]
  #
  # And the block form:
  #
  #    a.scan(/\w+/) {|w| print "<<#{w}>> " }
  #    print "\n"
  #    a.scan(/(.)(.)/) {|x,y| print y, x }
  #    print "\n"
  #
  # <em>produces:</em>
  #
  #    <<cruel>> <<world>>
  #    rceu lowlr
  def scan(pattern) end

  # If the string is invalid byte sequence then replace invalid bytes with given replacement
  # character, else returns self.
  # If block is given, replace invalid bytes with returned value of the block.
  #
  #    "abc\u3042\x81".scrub #=> "abc\u3042\uFFFD"
  #    "abc\u3042\x81".scrub("*") #=> "abc\u3042*"
  #    "abc\u3042\xE3\x80".scrub{|bytes| '<'+bytes.unpack('H*')[0]+'>' } #=> "abc\u3042<e380>"
  def scrub(*several_variants) end

  # If the string is invalid byte sequence then replace invalid bytes with given replacement
  # character, else returns self.
  # If block is given, replace invalid bytes with returned value of the block.
  #
  #    "abc\u3042\x81".scrub! #=> "abc\u3042\uFFFD"
  #    "abc\u3042\x81".scrub!("*") #=> "abc\u3042*"
  #    "abc\u3042\xE3\x80".scrub!{|bytes| '<'+bytes.unpack('H*')[0]+'>' } #=> "abc\u3042<e380>"
  def scrub!(*several_variants) end

  # modifies the <i>index</i>th byte as <i>integer</i>.
  def setbyte(index, integer) end

  # Deletes the specified portion from <i>str</i>, and returns the portion
  # deleted.
  #
  #    string = "this is a string"
  #    string.slice!(2)        #=> "i"
  #    string.slice!(3..6)     #=> " is "
  #    string.slice!(/s.*t/)   #=> "sa st"
  #    string.slice!("r")      #=> "r"
  #    string                  #=> "thing"
  def slice!(*several_variants) end

  # Divides <i>str</i> into substrings based on a delimiter, returning an array
  # of these substrings.
  #
  # If <i>pattern</i> is a <code>String</code>, then its contents are used as
  # the delimiter when splitting <i>str</i>. If <i>pattern</i> is a single
  # space, <i>str</i> is split on whitespace, with leading and trailing
  # whitespace and runs of contiguous whitespace characters ignored.
  #
  # If <i>pattern</i> is a <code>Regexp</code>, <i>str</i> is divided where the
  # pattern matches. Whenever the pattern matches a zero-length string,
  # <i>str</i> is split into individual characters. If <i>pattern</i> contains
  # groups, the respective matches will be returned in the array as well.
  #
  # If <i>pattern</i> is <code>nil</code>, the value of <code>$;</code> is used.
  # If <code>$;</code> is <code>nil</code> (which is the default), <i>str</i> is
  # split on whitespace as if ' ' were specified.
  #
  # If the <i>limit</i> parameter is omitted, trailing null fields are
  # suppressed. If <i>limit</i> is a positive number, at most that number
  # of split substrings will be returned (captured groups will be returned
  # as well, but are not counted towards the limit).
  # If <i>limit</i> is <code>1</code>, the entire
  # string is returned as the only entry in an array. If negative, there is no
  # limit to the number of fields returned, and trailing null fields are not
  # suppressed.
  #
  # When the input +str+ is empty an empty Array is returned as the string is
  # considered to have no fields to split.
  #
  #    " now's  the time ".split       #=> ["now's", "the", "time"]
  #    " now's  the time ".split(' ')  #=> ["now's", "the", "time"]
  #    " now's  the time".split(/ /)   #=> ["", "now's", "", "the", "time"]
  #    "1, 2.34,56, 7".split(%r{,\s*}) #=> ["1", "2.34", "56", "7"]
  #    "hello".split(//)               #=> ["h", "e", "l", "l", "o"]
  #    "hello".split(//, 3)            #=> ["h", "e", "llo"]
  #    "hi mom".split(%r{\s*})         #=> ["h", "i", "m", "o", "m"]
  #
  #    "mellow yellow".split("ello")   #=> ["m", "w y", "w"]
  #    "1,2,,3,4,,".split(',')         #=> ["1", "2", "", "3", "4"]
  #    "1,2,,3,4,,".split(',', 4)      #=> ["1", "2", "", "3,4,,"]
  #    "1,2,,3,4,,".split(',', -4)     #=> ["1", "2", "", "3", "4", "", ""]
  #
  #    "1:2:3".split(/(:)()()/, 2)     #=> ["1", ":", "", "", "2:3"]
  #
  #    "".split(',', -1)               #=> []
  #
  # If a block is given, invoke the block with each split substring.
  def split(pattern = nil, *limit) end

  # Builds a set of characters from the <i>other_str</i> parameter(s) using the
  # procedure described for <code>String#count</code>. Returns a new string
  # where runs of the same character that occur in this set are replaced by a
  # single character. If no arguments are given, all runs of identical
  # characters are replaced by a single character.
  #
  #    "yellow moon".squeeze                  #=> "yelow mon"
  #    "  now   is  the".squeeze(" ")         #=> " now is the"
  #    "putters shoot balls".squeeze("m-z")   #=> "puters shot balls"
  def squeeze(*other_strings) end

  # Squeezes <i>str</i> in place, returning either <i>str</i>, or
  # <code>nil</code> if no changes were made.
  def squeeze!(*other_strings) end

  # Returns true if +str+ starts with one of the +prefixes+ given.
  # Each of the +prefixes+ should be a String or a Regexp.
  #
  #   "hello".start_with?("hell")               #=> true
  #   "hello".start_with?(/H/i)                 #=> true
  #
  #   # returns true if one of the prefixes matches.
  #   "hello".start_with?("heaven", "hell")     #=> true
  #   "hello".start_with?("heaven", "paradise") #=> false
  def start_with?(*args) end

  # Returns a copy of the receiver with leading and trailing whitespace removed.
  #
  # Whitespace is defined as any of the following characters:
  # null, horizontal tab, line feed, vertical tab, form feed, carriage return, space.
  #
  #    "    hello    ".strip   #=> "hello"
  #    "\tgoodbye\r\n".strip   #=> "goodbye"
  #    "\x00\t\n\v\f\r ".strip #=> ""
  #    "hello".strip           #=> "hello"
  def strip; end

  # Removes leading and trailing whitespace from the receiver.
  # Returns the altered receiver, or +nil+ if there was no change.
  #
  # Refer to String#strip for the definition of whitespace.
  #
  #    "  hello  ".strip!  #=> "hello"
  #    "hello".strip!      #=> nil
  def strip!; end

  # Returns a copy of +str+ with the _first_ occurrence of +pattern+
  # replaced by the second argument. The +pattern+ is typically a Regexp; if
  # given as a String, any regular expression metacharacters it contains will
  # be interpreted literally, e.g. <code>'\\\d'</code> will match a backslash
  # followed by 'd', instead of a digit.
  #
  # If +replacement+ is a String it will be substituted for the matched text.
  # It may contain back-references to the pattern's capture groups of the form
  # <code>"\\d"</code>, where <i>d</i> is a group number, or
  # <code>"\\k<n>"</code>, where <i>n</i> is a group name. If it is a
  # double-quoted string, both back-references must be preceded by an
  # additional backslash. However, within +replacement+ the special match
  # variables, such as <code>$&</code>, will not refer to the current match.
  # If +replacement+ is a String that looks like a pattern's capture group but
  # is actually not a pattern capture group e.g. <code>"\\'"</code>, then it
  # will have to be preceded by two backslashes like so <code>"\\\\'"</code>.
  #
  # If the second argument is a Hash, and the matched text is one of its keys,
  # the corresponding value is the replacement string.
  #
  # In the block form, the current match string is passed in as a parameter,
  # and variables such as <code>$1</code>, <code>$2</code>, <code>$`</code>,
  # <code>$&</code>, and <code>$'</code> will be set appropriately. The value
  # returned by the block will be substituted for the match on each call.
  #
  # The result inherits any tainting in the original string or any supplied
  # replacement string.
  #
  #    "hello".sub(/[aeiou]/, '*')                  #=> "h*llo"
  #    "hello".sub(/([aeiou])/, '<\1>')             #=> "h<e>llo"
  #    "hello".sub(/./) {|s| s.ord.to_s + ' ' }     #=> "104 ello"
  #    "hello".sub(/(?<foo>[aeiou])/, '*\k<foo>*')  #=> "h*e*llo"
  #    'Is SHELL your preferred shell?'.sub(/[[:upper:]]{2,}/, ENV)
  #     #=> "Is /bin/bash your preferred shell?"
  def sub(*several_variants) end

  # Performs the same substitution as String#sub in-place.
  #
  # Returns +str+ if a substitution was performed or +nil+ if no substitution
  # was performed.
  def sub!(*several_variants) end

  # Returns the successor to <i>str</i>. The successor is calculated by
  # incrementing characters starting from the rightmost alphanumeric (or
  # the rightmost character if there are no alphanumerics) in the
  # string. Incrementing a digit always results in another digit, and
  # incrementing a letter results in another letter of the same case.
  # Incrementing nonalphanumerics uses the underlying character set's
  # collating sequence.
  #
  # If the increment generates a ``carry,'' the character to the left of
  # it is incremented. This process repeats until there is no carry,
  # adding an additional character if necessary.
  #
  #    "abcd".succ        #=> "abce"
  #    "THX1138".succ     #=> "THX1139"
  #    "<<koala>>".succ   #=> "<<koalb>>"
  #    "1999zzz".succ     #=> "2000aaa"
  #    "ZZZ9999".succ     #=> "AAAA0000"
  #    "***".succ         #=> "**+"
  def succ; end
  alias next succ

  # Equivalent to <code>String#succ</code>, but modifies the receiver in
  # place.
  def succ!; end
  alias next! succ!

  # Returns a basic <em>n</em>-bit checksum of the characters in <i>str</i>,
  # where <em>n</em> is the optional <code>Integer</code> parameter, defaulting
  # to 16. The result is simply the sum of the binary value of each byte in
  # <i>str</i> modulo <code>2**n - 1</code>. This is not a particularly good
  # checksum.
  def sum(n = 16) end

  # Returns a copy of <i>str</i> with uppercase alphabetic characters converted
  # to lowercase and lowercase characters converted to uppercase.
  #
  # See String#downcase for meaning of +options+ and use with different encodings.
  #
  #    "Hello".swapcase          #=> "hELLO"
  #    "cYbEr_PuNk11".swapcase   #=> "CyBeR_pUnK11"
  def swapcase(*several_variants) end

  # Equivalent to <code>String#swapcase</code>, but modifies the receiver in
  # place, returning <i>str</i>, or <code>nil</code> if no changes were made.
  #
  # See String#downcase for meaning of +options+ and use with different encodings.
  def swapcase!(*several_variants) end

  # Returns a complex which denotes the string form.  The parser
  # ignores leading whitespaces and trailing garbage.  Any digit
  # sequences can be separated by an underscore.  Returns zero for null
  # or garbage string.
  #
  #    '9'.to_c           #=> (9+0i)
  #    '2.5'.to_c         #=> (2.5+0i)
  #    '2.5/1'.to_c       #=> ((5/2)+0i)
  #    '-3/2'.to_c        #=> ((-3/2)+0i)
  #    '-i'.to_c          #=> (0-1i)
  #    '45i'.to_c         #=> (0+45i)
  #    '3-4i'.to_c        #=> (3-4i)
  #    '-4e2-4e-2i'.to_c  #=> (-400.0-0.04i)
  #    '-0.0-0.0i'.to_c   #=> (-0.0-0.0i)
  #    '1/2+3/4i'.to_c    #=> ((1/2)+(3/4)*i)
  #    'ruby'.to_c        #=> (0+0i)
  #
  # See Kernel.Complex.
  def to_c; end

  # Returns the result of interpreting leading characters in <i>str</i> as a
  # floating point number. Extraneous characters past the end of a valid number
  # are ignored. If there is not a valid number at the start of <i>str</i>,
  # <code>0.0</code> is returned. This method never raises an exception.
  #
  #    "123.45e1".to_f        #=> 1234.5
  #    "45.67 degrees".to_f   #=> 45.67
  #    "thx1138".to_f         #=> 0.0
  def to_f; end

  # Returns the result of interpreting leading characters in <i>str</i> as an
  # integer base <i>base</i> (between 2 and 36). Extraneous characters past the
  # end of a valid number are ignored. If there is not a valid number at the
  # start of <i>str</i>, <code>0</code> is returned. This method never raises an
  # exception when <i>base</i> is valid.
  #
  #    "12345".to_i             #=> 12345
  #    "99 red balloons".to_i   #=> 99
  #    "0a".to_i                #=> 0
  #    "0a".to_i(16)            #=> 10
  #    "hello".to_i             #=> 0
  #    "1100101".to_i(2)        #=> 101
  #    "1100101".to_i(8)        #=> 294977
  #    "1100101".to_i(10)       #=> 1100101
  #    "1100101".to_i(16)       #=> 17826049
  def to_i(base = 10) end

  # Returns the result of interpreting leading characters in +str+
  # as a rational.  Leading whitespace and extraneous characters
  # past the end of a valid number are ignored.
  # Digit sequences can be separated by an underscore.
  # If there is not a valid number at the start of +str+,
  # zero is returned.  This method never raises an exception.
  #
  #    '  2  '.to_r       #=> (2/1)
  #    '300/2'.to_r       #=> (150/1)
  #    '-9.2'.to_r        #=> (-46/5)
  #    '-9.2e2'.to_r      #=> (-920/1)
  #    '1_234_567'.to_r   #=> (1234567/1)
  #    '21 June 09'.to_r  #=> (21/1)
  #    '21/06/09'.to_r    #=> (7/2)
  #    'BWV 1079'.to_r    #=> (0/1)
  #
  # NOTE: "0.3".to_r isn't the same as 0.3.to_r.  The former is
  # equivalent to "3/10".to_r, but the latter isn't so.
  #
  #    "0.3".to_r == 3/10r  #=> true
  #    0.3.to_r   == 3/10r  #=> false
  #
  # See also Kernel#Rational.
  def to_r; end

  # Returns +self+.
  #
  # If called on a subclass of String, converts the receiver to a String object.
  def to_s; end
  alias to_str to_s

  # Returns a copy of +str+ with the characters in +from_str+ replaced by the
  # corresponding characters in +to_str+.  If +to_str+ is shorter than
  # +from_str+, it is padded with its last character in order to maintain the
  # correspondence.
  #
  #    "hello".tr('el', 'ip')      #=> "hippo"
  #    "hello".tr('aeiou', '*')    #=> "h*ll*"
  #    "hello".tr('aeiou', 'AA*')  #=> "hAll*"
  #
  # Both strings may use the <code>c1-c2</code> notation to denote ranges of
  # characters, and +from_str+ may start with a <code>^</code>, which denotes
  # all characters except those listed.
  #
  #    "hello".tr('a-y', 'b-z')    #=> "ifmmp"
  #    "hello".tr('^aeiou', '*')   #=> "*e**o"
  #
  # The backslash character <code>\\</code> can be used to escape
  # <code>^</code> or <code>-</code> and is otherwise ignored unless it
  # appears at the end of a range or the end of the +from_str+ or +to_str+:
  #
  #    "hello^world".tr("\\^aeiou", "*") #=> "h*ll**w*rld"
  #    "hello-world".tr("a\\-eo", "*")   #=> "h*ll**w*rld"
  #
  #    "hello\r\nworld".tr("\r", "")   #=> "hello\nworld"
  #    "hello\r\nworld".tr("\\r", "")  #=> "hello\r\nwold"
  #    "hello\r\nworld".tr("\\\r", "") #=> "hello\nworld"
  #
  #    "X['\\b']".tr("X\\", "")   #=> "['b']"
  #    "X['\\b']".tr("X-\\]", "") #=> "'b'"
  def tr(from_str, to_str) end

  # Translates <i>str</i> in place, using the same rules as
  # <code>String#tr</code>. Returns <i>str</i>, or <code>nil</code> if no
  # changes were made.
  def tr!(from_str, to_str) end

  # Processes a copy of <i>str</i> as described under <code>String#tr</code>,
  # then removes duplicate characters in regions that were affected by the
  # translation.
  #
  #    "hello".tr_s('l', 'r')     #=> "hero"
  #    "hello".tr_s('el', '*')    #=> "h*o"
  #    "hello".tr_s('el', 'hx')   #=> "hhxo"
  def tr_s(from_str, to_str) end

  # Performs <code>String#tr_s</code> processing on <i>str</i> in place,
  # returning <i>str</i>, or <code>nil</code> if no changes were made.
  def tr_s!(from_str, to_str) end

  # Produces unescaped version of +str+.
  # See also String#dump because String#undump does inverse of String#dump.
  #
  #   "\"hello \\n ''\"".undump #=> "hello \n ''"
  def undump; end

  # Unicode Normalization---Returns a normalized form of +str+,
  # using Unicode normalizations NFC, NFD, NFKC, or NFKD.
  # The normalization form used is determined by +form+, which can
  # be any of the four values +:nfc+, +:nfd+, +:nfkc+, or +:nfkd+.
  # The default is +:nfc+.
  #
  # If the string is not in a Unicode Encoding, then an Exception is raised.
  # In this context, 'Unicode Encoding' means any of UTF-8, UTF-16BE/LE,
  # and UTF-32BE/LE, as well as GB18030, UCS_2BE, and UCS_4BE.
  # Anything other than UTF-8 is implemented by converting to UTF-8,
  # which makes it slower than UTF-8.
  #
  #   "a\u0300".unicode_normalize        #=> "\u00E0"
  #   "a\u0300".unicode_normalize(:nfc)  #=> "\u00E0"
  #   "\u00E0".unicode_normalize(:nfd)   #=> "a\u0300"
  #   "\xE0".force_encoding('ISO-8859-1').unicode_normalize(:nfd)
  #                                      #=> Encoding::CompatibilityError raised
  def unicode_normalize(form = :nfc) end

  # Destructive version of String#unicode_normalize, doing Unicode
  # normalization in place.
  def unicode_normalize!(form = :nfc) end

  # Checks whether +str+ is in Unicode normalization form +form+,
  # which can be any of the four values +:nfc+, +:nfd+, +:nfkc+, or +:nfkd+.
  # The default is +:nfc+.
  #
  # If the string is not in a Unicode Encoding, then an Exception is raised.
  # For details, see String#unicode_normalize.
  #
  #   "a\u0300".unicode_normalized?        #=> false
  #   "a\u0300".unicode_normalized?(:nfd)  #=> true
  #   "\u00E0".unicode_normalized?         #=> true
  #   "\u00E0".unicode_normalized?(:nfd)   #=> false
  #   "\xE0".force_encoding('ISO-8859-1').unicode_normalized?
  #                                        #=> Encoding::CompatibilityError raised
  def unicode_normalized?(form = :nfc) end

  # Decodes <i>str</i> (which may contain binary data) according to the
  # format string, returning an array of each value extracted. The
  # format string consists of a sequence of single-character directives,
  # summarized in the table at the end of this entry.
  # Each directive may be followed
  # by a number, indicating the number of times to repeat with this
  # directive. An asterisk (``<code>*</code>'') will use up all
  # remaining elements. The directives <code>sSiIlL</code> may each be
  # followed by an underscore (``<code>_</code>'') or
  # exclamation mark (``<code>!</code>'') to use the underlying
  # platform's native size for the specified type; otherwise, it uses a
  # platform-independent consistent size. Spaces are ignored in the
  # format string. See also <code>String#unpack1</code>,  <code>Array#pack</code>.
  #
  #    "abc \0\0abc \0\0".unpack('A6Z6')   #=> ["abc", "abc "]
  #    "abc \0\0".unpack('a3a3')           #=> ["abc", " \000\000"]
  #    "abc \0abc \0".unpack('Z*Z*')       #=> ["abc ", "abc "]
  #    "aa".unpack('b8B8')                 #=> ["10000110", "01100001"]
  #    "aaa".unpack('h2H2c')               #=> ["16", "61", 97]
  #    "\xfe\xff\xfe\xff".unpack('sS')     #=> [-2, 65534]
  #    "now=20is".unpack('M*')             #=> ["now is"]
  #    "whole".unpack('xax2aX2aX1aX2a')    #=> ["h", "e", "l", "l", "o"]
  #
  # This table summarizes the various formats and the Ruby classes
  # returned by each.
  #
  #  Integer       |         |
  #  Directive     | Returns | Meaning
  #  ------------------------------------------------------------------
  #  C             | Integer | 8-bit unsigned (unsigned char)
  #  S             | Integer | 16-bit unsigned, native endian (uint16_t)
  #  L             | Integer | 32-bit unsigned, native endian (uint32_t)
  #  Q             | Integer | 64-bit unsigned, native endian (uint64_t)
  #  J             | Integer | pointer width unsigned, native endian (uintptr_t)
  #                |         |
  #  c             | Integer | 8-bit signed (signed char)
  #  s             | Integer | 16-bit signed, native endian (int16_t)
  #  l             | Integer | 32-bit signed, native endian (int32_t)
  #  q             | Integer | 64-bit signed, native endian (int64_t)
  #  j             | Integer | pointer width signed, native endian (intptr_t)
  #                |         |
  #  S_ S!         | Integer | unsigned short, native endian
  #  I I_ I!       | Integer | unsigned int, native endian
  #  L_ L!         | Integer | unsigned long, native endian
  #  Q_ Q!         | Integer | unsigned long long, native endian (ArgumentError
  #                |         | if the platform has no long long type.)
  #  J!            | Integer | uintptr_t, native endian (same with J)
  #                |         |
  #  s_ s!         | Integer | signed short, native endian
  #  i i_ i!       | Integer | signed int, native endian
  #  l_ l!         | Integer | signed long, native endian
  #  q_ q!         | Integer | signed long long, native endian (ArgumentError
  #                |         | if the platform has no long long type.)
  #  j!            | Integer | intptr_t, native endian (same with j)
  #                |         |
  #  S> s> S!> s!> | Integer | same as the directives without ">" except
  #  L> l> L!> l!> |         | big endian
  #  I!> i!>       |         |
  #  Q> q> Q!> q!> |         | "S>" is same as "n"
  #  J> j> J!> j!> |         | "L>" is same as "N"
  #                |         |
  #  S< s< S!< s!< | Integer | same as the directives without "<" except
  #  L< l< L!< l!< |         | little endian
  #  I!< i!<       |         |
  #  Q< q< Q!< q!< |         | "S<" is same as "v"
  #  J< j< J!< j!< |         | "L<" is same as "V"
  #                |         |
  #  n             | Integer | 16-bit unsigned, network (big-endian) byte order
  #  N             | Integer | 32-bit unsigned, network (big-endian) byte order
  #  v             | Integer | 16-bit unsigned, VAX (little-endian) byte order
  #  V             | Integer | 32-bit unsigned, VAX (little-endian) byte order
  #                |         |
  #  U             | Integer | UTF-8 character
  #  w             | Integer | BER-compressed integer (see Array.pack)
  #
  #  Float        |         |
  #  Directive    | Returns | Meaning
  #  -----------------------------------------------------------------
  #  D d          | Float   | double-precision, native format
  #  F f          | Float   | single-precision, native format
  #  E            | Float   | double-precision, little-endian byte order
  #  e            | Float   | single-precision, little-endian byte order
  #  G            | Float   | double-precision, network (big-endian) byte order
  #  g            | Float   | single-precision, network (big-endian) byte order
  #
  #  String       |         |
  #  Directive    | Returns | Meaning
  #  -----------------------------------------------------------------
  #  A            | String  | arbitrary binary string (remove trailing nulls and ASCII spaces)
  #  a            | String  | arbitrary binary string
  #  Z            | String  | null-terminated string
  #  B            | String  | bit string (MSB first)
  #  b            | String  | bit string (LSB first)
  #  H            | String  | hex string (high nibble first)
  #  h            | String  | hex string (low nibble first)
  #  u            | String  | UU-encoded string
  #  M            | String  | quoted-printable, MIME encoding (see RFC2045)
  #  m            | String  | base64 encoded string (RFC 2045) (default)
  #               |         | base64 encoded string (RFC 4648) if followed by 0
  #  P            | String  | pointer to a structure (fixed-length string)
  #  p            | String  | pointer to a null-terminated string
  #
  #  Misc.        |         |
  #  Directive    | Returns | Meaning
  #  -----------------------------------------------------------------
  #  @            | ---     | skip to the offset given by the length argument
  #  X            | ---     | skip backward one byte
  #  x            | ---     | skip forward one byte
  #
  # HISTORY
  #
  # * J, J! j, and j! are available since Ruby 2.3.
  # * Q_, Q!, q_, and q! are available since Ruby 2.1.
  # * I!<, i!<, I!>, and i!> are available since Ruby 1.9.3.
  def unpack(format) end

  # Decodes <i>str</i> (which may contain binary data) according to the
  # format string, returning the first value extracted.
  # See also <code>String#unpack</code>, <code>Array#pack</code>.
  def unpack1(format) end

  # Returns a copy of <i>str</i> with all lowercase letters replaced with their
  # uppercase counterparts.
  #
  # See String#downcase for meaning of +options+ and use with different encodings.
  #
  #    "hEllO".upcase   #=> "HELLO"
  def upcase(*several_variants) end

  # Upcases the contents of <i>str</i>, returning <code>nil</code> if no changes
  # were made.
  #
  # See String#downcase for meaning of +options+ and use with different encodings.
  def upcase!(*several_variants) end

  # Iterates through successive values, starting at <i>str</i> and
  # ending at <i>other_str</i> inclusive, passing each value in turn to
  # the block. The <code>String#succ</code> method is used to generate
  # each value.  If optional second argument exclusive is omitted or is false,
  # the last value will be included; otherwise it will be excluded.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    "a8".upto("b6") {|s| print s, ' ' }
  #    for s in "a8".."b6"
  #      print s, ' '
  #    end
  #
  # <em>produces:</em>
  #
  #    a8 a9 b0 b1 b2 b3 b4 b5 b6
  #    a8 a9 b0 b1 b2 b3 b4 b5 b6
  #
  # If <i>str</i> and <i>other_str</i> contains only ascii numeric characters,
  # both are recognized as decimal numbers. In addition, the width of
  # string (e.g. leading zeros) is handled appropriately.
  #
  #    "9".upto("11").to_a   #=> ["9", "10", "11"]
  #    "25".upto("5").to_a   #=> []
  #    "07".upto("11").to_a  #=> ["07", "08", "09", "10", "11"]
  def upto(other_str, exclusive = false) end

  # Returns true for a string which is encoded correctly.
  #
  #   "\xc2\xa1".force_encoding("UTF-8").valid_encoding?  #=> true
  #   "\xc2".force_encoding("UTF-8").valid_encoding?      #=> false
  #   "\x80".force_encoding("UTF-8").valid_encoding?      #=> false
  def valid_encoding?; end
end
