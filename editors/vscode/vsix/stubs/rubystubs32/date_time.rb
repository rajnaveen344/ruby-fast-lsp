# frozen_string_literal: true

# == DateTime
#
# A subclass of Date that easily handles date, hour, minute, second,
# and offset.
#
# DateTime class is considered deprecated. Use Time class.
#
# DateTime does not consider any leap seconds, does not track
# any summer time rules.
#
# A DateTime object is created with DateTime::new, DateTime::jd,
# DateTime::ordinal, DateTime::commercial, DateTime::parse,
# DateTime::strptime, DateTime::now, Time#to_datetime, etc.
#
#     require 'date'
#
#     DateTime.new(2001,2,3,4,5,6)
#                         #=> #<DateTime: 2001-02-03T04:05:06+00:00 ...>
#
# The last element of day, hour, minute, or second can be a
# fractional number. The fractional number's precision is assumed
# at most nanosecond.
#
#     DateTime.new(2001,2,3.5)
#                         #=> #<DateTime: 2001-02-03T12:00:00+00:00 ...>
#
# An optional argument, the offset, indicates the difference
# between the local time and UTC. For example, <tt>Rational(3,24)</tt>
# represents ahead of 3 hours of UTC, <tt>Rational(-5,24)</tt> represents
# behind of 5 hours of UTC. The offset should be -1 to +1, and
# its precision is assumed at most second. The default value is
# zero (equals to UTC).
#
#     DateTime.new(2001,2,3,4,5,6,Rational(3,24))
#                         #=> #<DateTime: 2001-02-03T04:05:06+03:00 ...>
#
# The offset also accepts string form:
#
#     DateTime.new(2001,2,3,4,5,6,'+03:00')
#                         #=> #<DateTime: 2001-02-03T04:05:06+03:00 ...>
#
# An optional argument, the day of calendar reform (+start+), denotes
# a Julian day number, which should be 2298874 to 2426355 or
# negative/positive infinity.
# The default value is +Date::ITALY+ (2299161=1582-10-15).
#
# A DateTime object has various methods. See each reference.
#
#     d = DateTime.parse('3rd Feb 2001 04:05:06+03:30')
#                         #=> #<DateTime: 2001-02-03T04:05:06+03:30 ...>
#     d.hour              #=> 4
#     d.min               #=> 5
#     d.sec               #=> 6
#     d.offset            #=> (7/48)
#     d.zone              #=> "+03:30"
#     d += Rational('1.5')
#                         #=> #<DateTime: 2001-02-04%16:05:06+03:30 ...>
#     d = d.new_offset('+09:00')
#                         #=> #<DateTime: 2001-02-04%21:35:06+09:00 ...>
#     d.strftime('%I:%M:%S %p')
#                         #=> "09:35:06 PM"
#     d > DateTime.new(1999)
#                         #=> true
#
# === When should you use DateTime and when should you use Time?
#
# It's a common misconception that
# {William Shakespeare}[https://en.wikipedia.org/wiki/William_Shakespeare]
# and
# {Miguel de Cervantes}[https://en.wikipedia.org/wiki/Miguel_de_Cervantes]
# died on the same day in history -
# so much so that UNESCO named April 23 as
# {World Book Day because of this fact}[https://en.wikipedia.org/wiki/World_Book_Day].
# However, because England hadn't yet adopted the
# {Gregorian Calendar Reform}[https://en.wikipedia.org/wiki/Gregorian_calendar#Gregorian_reform]
# (and wouldn't until {1752}[https://en.wikipedia.org/wiki/Calendar_(New_Style)_Act_1750])
# their deaths are actually 10 days apart.
# Since Ruby's Time class implements a
# {proleptic Gregorian calendar}[https://en.wikipedia.org/wiki/Proleptic_Gregorian_calendar]
# and has no concept of calendar reform there's no way
# to express this with Time objects. This is where DateTime steps in:
#
#     shakespeare = DateTime.iso8601('1616-04-23', Date::ENGLAND)
#      #=> Tue, 23 Apr 1616 00:00:00 +0000
#     cervantes = DateTime.iso8601('1616-04-23', Date::ITALY)
#      #=> Sat, 23 Apr 1616 00:00:00 +0000
#
# Already you can see something is weird - the days of the week
# are different. Taking this further:
#
#     cervantes == shakespeare
#      #=> false
#     (shakespeare - cervantes).to_i
#      #=> 10
#
# This shows that in fact they died 10 days apart (in reality
# 11 days since Cervantes died a day earlier but was buried on
# the 23rd). We can see the actual date of Shakespeare's death by
# using the #gregorian method to convert it:
#
#     shakespeare.gregorian
#      #=> Tue, 03 May 1616 00:00:00 +0000
#
# So there's an argument that all the celebrations that take
# place on the 23rd April in Stratford-upon-Avon are actually
# the wrong date since England is now using the Gregorian calendar.
# You can see why when we transition across the reform
# date boundary:
#
#     # start off with the anniversary of Shakespeare's birth in 1751
#     shakespeare = DateTime.iso8601('1751-04-23', Date::ENGLAND)
#      #=> Tue, 23 Apr 1751 00:00:00 +0000
#
#     # add 366 days since 1752 is a leap year and April 23 is after February 29
#     shakespeare + 366
#      #=> Thu, 23 Apr 1752 00:00:00 +0000
#
#     # add another 365 days to take us to the anniversary in 1753
#     shakespeare + 366 + 365
#      #=> Fri, 04 May 1753 00:00:00 +0000
#
# As you can see, if we're accurately tracking the number of
# {solar years}[https://en.wikipedia.org/wiki/Tropical_year]
# since Shakespeare's birthday then the correct anniversary date
# would be the 4th May and not the 23rd April.
#
# So when should you use DateTime in Ruby and when should
# you use Time? Almost certainly you'll want to use Time
# since your app is probably dealing with current dates and
# times. However, if you need to deal with dates and times in a
# historical context you'll want to use DateTime to avoid
# making the same mistakes as UNESCO. If you also have to deal
# with timezones then best of luck - just bear in mind that
# you'll probably be dealing with
# {local solar times}[https://en.wikipedia.org/wiki/Solar_time],
# since it wasn't until the 19th century that the introduction
# of the railways necessitated the need for
# {Standard Time}[https://en.wikipedia.org/wiki/Standard_time#Great_Britain]
# and eventually timezones.
class DateTime < Date
  # Parses the given representation of date and time with the given
  # template, and returns a hash of parsed elements.  _strptime does
  # not support specification of flags and width unlike strftime.
  #
  # See also strptime(3) and #strftime.
  def self._strptime(*args) end

  # Same as DateTime.new.
  def self.civil(*args) end

  # Creates a DateTime object denoting the given week date.
  #
  #    DateTime.commercial(2001) #=> #<DateTime: 2001-01-01T00:00:00+00:00 ...>
  #    DateTime.commercial(2002) #=> #<DateTime: 2001-12-31T00:00:00+00:00 ...>
  #    DateTime.commercial(2001,5,6,4,5,6,'+7')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  def self.commercial(p1 = v1, p2 = v2, p3 = v3, p4 = v4, p5 = v5, p6 = v6, p7 = v7, p8 = v8) end

  # Creates a new DateTime object by parsing from a string according to
  # some RFC 2616 format.
  #
  #    DateTime.httpdate('Sat, 03 Feb 2001 04:05:06 GMT')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+00:00 ...>
  #
  # Raise an ArgumentError when the string length is longer than _limit_.
  # You can stop this check by passing <code>limit: nil</code>, but note
  # that it may take a long time to parse.
  def self.httpdate(p1 = v1, p2 = v2, p3 = {}) end

  # Creates a new DateTime object by parsing from a string according to
  # some typical ISO 8601 formats.
  #
  #    DateTime.iso8601('2001-02-03T04:05:06+07:00')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.iso8601('20010203T040506+0700')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.iso8601('2001-W05-6T04:05:06+07:00')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #
  # Raise an ArgumentError when the string length is longer than _limit_.
  # You can stop this check by passing <code>limit: nil</code>, but note
  # that it may take a long time to parse.
  def self.iso8601(p1 = v1, p2 = v2, p3 = {}) end

  # Creates a DateTime object denoting the given chronological Julian
  # day number.
  #
  #    DateTime.jd(2451944)      #=> #<DateTime: 2001-02-03T00:00:00+00:00 ...>
  #    DateTime.jd(2451945)      #=> #<DateTime: 2001-02-04T00:00:00+00:00 ...>
  #    DateTime.jd(Rational('0.5'))
  #                              #=> #<DateTime: -4712-01-01T12:00:00+00:00 ...>
  def self.jd(p1 = v1, p2 = v2, p3 = v3, p4 = v4, p5 = v5, p6 = v6) end

  # Creates a new DateTime object by parsing from a string according to
  # some typical JIS X 0301 formats.
  #
  #    DateTime.jisx0301('H13.02.03T04:05:06+07:00')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #
  # For no-era year, legacy format, Heisei is assumed.
  #
  #    DateTime.jisx0301('13.02.03T04:05:06+07:00')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #
  # Raise an ArgumentError when the string length is longer than _limit_.
  # You can stop this check by passing <code>limit: nil</code>, but note
  # that it may take a long time to parse.
  def self.jisx0301(p1 = v1, p2 = v2, p3 = {}) end

  # Creates a DateTime object denoting the present time.
  #
  #    DateTime.now              #=> #<DateTime: 2011-06-11T21:20:44+09:00 ...>
  def self.now(p1 = v1) end

  # Creates a DateTime object denoting the given ordinal date.
  #
  #    DateTime.ordinal(2001,34) #=> #<DateTime: 2001-02-03T00:00:00+00:00 ...>
  #    DateTime.ordinal(2001,34,4,5,6,'+7')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.ordinal(2001,-332,-20,-55,-54,'+7')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  def self.ordinal(p1 = v1, p2 = v2, p3 = v3, p4 = v4, p5 = v5, p6 = v6, p7 = v7) end

  # Parses the given representation of date and time, and creates a
  # DateTime object.
  #
  # This method *does* *not* function as a validator.  If the input
  # string does not match valid formats strictly, you may get a cryptic
  # result.  Should consider to use DateTime.strptime instead of this
  # method as possible.
  #
  # If the optional second argument is true and the detected year is in
  # the range "00" to "99", makes it full.
  #
  #    DateTime.parse('2001-02-03T04:05:06+07:00')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.parse('20010203T040506+0700')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.parse('3rd Feb 2001 04:05:06 PM')
  #                              #=> #<DateTime: 2001-02-03T16:05:06+00:00 ...>
  #
  # Raise an ArgumentError when the string length is longer than _limit_.
  # You can stop this check by passing <code>limit: nil</code>, but note
  # that it may take a long time to parse.
  def self.parse(p1 = v1, p2 = v2, p3 = v3, p4 = {}) end

  # Creates a new DateTime object by parsing from a string according to
  # some typical RFC 2822 formats.
  #
  #     DateTime.rfc2822('Sat, 3 Feb 2001 04:05:06 +0700')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #
  # Raise an ArgumentError when the string length is longer than _limit_.
  # You can stop this check by passing <code>limit: nil</code>, but note
  # that it may take a long time to parse.
  def self.rfc2822(p1 = v1, p2 = v2, p3 = {}) end

  # Creates a new DateTime object by parsing from a string according to
  # some typical RFC 3339 formats.
  #
  #    DateTime.rfc3339('2001-02-03T04:05:06+07:00')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #
  # Raise an ArgumentError when the string length is longer than _limit_.
  # You can stop this check by passing <code>limit: nil</code>, but note
  # that it may take a long time to parse.
  def self.rfc3339(p1 = v1, p2 = v2, p3 = {}) end

  # Creates a new DateTime object by parsing from a string according to
  # some typical RFC 2822 formats.
  #
  #     DateTime.rfc2822('Sat, 3 Feb 2001 04:05:06 +0700')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #
  # Raise an ArgumentError when the string length is longer than _limit_.
  # You can stop this check by passing <code>limit: nil</code>, but note
  # that it may take a long time to parse.
  def self.rfc822(p1 = v1, p2 = v2, p3 = {}) end

  # Parses the given representation of date and time with the given
  # template, and creates a DateTime object.  strptime does not support
  # specification of flags and width unlike strftime.
  #
  #    DateTime.strptime('2001-02-03T04:05:06+07:00', '%Y-%m-%dT%H:%M:%S%z')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.strptime('03-02-2001 04:05:06 PM', '%d-%m-%Y %I:%M:%S %p')
  #                              #=> #<DateTime: 2001-02-03T16:05:06+00:00 ...>
  #    DateTime.strptime('2001-W05-6T04:05:06+07:00', '%G-W%V-%uT%H:%M:%S%z')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.strptime('2001 04 6 04 05 06 +7', '%Y %U %w %H %M %S %z')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.strptime('2001 05 6 04 05 06 +7', '%Y %W %u %H %M %S %z')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #    DateTime.strptime('-1', '%s')
  #                              #=> #<DateTime: 1969-12-31T23:59:59+00:00 ...>
  #    DateTime.strptime('-1000', '%Q')
  #                              #=> #<DateTime: 1969-12-31T23:59:59+00:00 ...>
  #    DateTime.strptime('sat3feb014pm+7', '%a%d%b%y%H%p%z')
  #                              #=> #<DateTime: 2001-02-03T16:00:00+07:00 ...>
  #
  # See also strptime(3) and #strftime.
  def self.strptime(p1 = v1, p2 = v2, p3 = v3) end

  # Creates a new DateTime object by parsing from a string according to
  # some typical XML Schema formats.
  #
  #    DateTime.xmlschema('2001-02-03T04:05:06+07:00')
  #                              #=> #<DateTime: 2001-02-03T04:05:06+07:00 ...>
  #
  # Raise an ArgumentError when the string length is longer than _limit_.
  # You can stop this check by passing <code>limit: nil</code>, but note
  # that it may take a long time to parse.
  def self.xmlschema(p1 = v1, p2 = v2, p3 = {}) end

  # Same as DateTime.new.
  def initialize(*args) end

  # Returns a hash of the name/value pairs, to use in pattern matching.
  # Possible keys are: <tt>:year</tt>, <tt>:month</tt>, <tt>:day</tt>,
  # <tt>:wday</tt>, <tt>:yday</tt>, <tt>:hour</tt>, <tt>:min</tt>,
  # <tt>:sec</tt>, <tt>:sec_fraction</tt>, <tt>:zone</tt>.
  #
  # Possible usages:
  #
  #   dt = DateTime.new(2022, 10, 5, 13, 30)
  #
  #   if d in wday: 1..5, hour: 10..18  # uses deconstruct_keys underneath
  #     puts "Working time"
  #   end
  #   #=> prints "Working time"
  #
  #   case dt
  #   in year: ...2022
  #     puts "too old"
  #   in month: ..9
  #     puts "quarter 1-3"
  #   in wday: 1..5, month:
  #     puts "working day in month #{month}"
  #   end
  #   #=> prints "working day in month 10"
  #
  # Note that deconstruction by pattern can also be combined with class check:
  #
  #   if d in DateTime(wday: 1..5, hour: 10..18, day: ..7)
  #     puts "Working time, first week of the month"
  #   end
  def deconstruct_keys(array_of_names_or_nil) end

  # Returns the hour in range (0..23):
  #
  #   DateTime.new(2001, 2, 3, 4, 5, 6).hour # => 4
  def hour; end

  # This method is equivalent to strftime('%FT%T%:z').
  # The optional argument +n+ is the number of digits for fractional seconds.
  #
  #    DateTime.parse('2001-02-03T04:05:06.123456789+07:00').iso8601(9)
  #                              #=> "2001-02-03T04:05:06.123456789+07:00"
  def iso8601(*args) end
  alias xmlschema iso8601

  # Returns a string in a JIS X 0301 format.
  # The optional argument +n+ is the number of digits for fractional seconds.
  #
  #    DateTime.parse('2001-02-03T04:05:06.123456789+07:00').jisx0301(9)
  #                              #=> "H13.02.03T04:05:06.123456789+07:00"
  def jisx0301(*args) end

  # Returns the minute in range (0..59):
  #
  #   DateTime.new(2001, 2, 3, 4, 5, 6).min # => 5
  #
  # Date#minute is an alias for Date#min.
  def min; end
  alias minute min

  # Duplicates self and resets its offset.
  #
  #    d = DateTime.new(2001,2,3,4,5,6,'-02:00')
  #                              #=> #<DateTime: 2001-02-03T04:05:06-02:00 ...>
  #    d.new_offset('+09:00')    #=> #<DateTime: 2001-02-03T15:05:06+09:00 ...>
  def new_offset(p1 = v1) end

  # Returns the offset.
  #
  #    DateTime.parse('04pm+0730').offset        #=> (5/16)
  def offset; end

  # This method is equivalent to strftime('%FT%T%:z').
  # The optional argument +n+ is the number of digits for fractional seconds.
  #
  #    DateTime.parse('2001-02-03T04:05:06.123456789+07:00').rfc3339(9)
  #                              #=> "2001-02-03T04:05:06.123456789+07:00"
  def rfc3339(*args) end

  # Returns the second in range (0..59):
  #
  #   DateTime.new(2001, 2, 3, 4, 5, 6).sec # => 6
  #
  # Date#second is an alias for Date#sec.
  def sec; end
  alias second sec

  # Returns the fractional part of the second in range
  # (Rational(0, 1)...Rational(1, 1)):
  #
  #   DateTime.new(2001, 2, 3, 4, 5, 6.5).sec_fraction # => (1/2)
  #
  # Date#second_fraction is an alias for Date#sec_fraction.
  def sec_fraction; end
  alias second_fraction sec_fraction

  # Returns a string representation of +self+,
  # formatted according the given +format:
  #
  #   DateTime.now.strftime # => "2022-07-01T11:03:19-05:00"
  #
  # For other formats, see
  # {Formats for Dates and Times}[doc/strftime_formatting.rdoc].
  def strftime(format = '%FT%T%:z') end

  # Returns a Date object which denotes self.
  def to_date; end

  # Returns self.
  def to_datetime; end

  # Returns a string in an ISO 8601 format. (This method doesn't use the
  # expanded representations.)
  #
  #     DateTime.new(2001,2,3,4,5,6,'-7').to_s
  #                              #=> "2001-02-03T04:05:06-07:00"
  def to_s; end

  # Returns a Time object which denotes self.
  def to_time; end

  # Returns the timezone.
  #
  #    DateTime.parse('04pm+0730').zone          #=> "+07:30"
  def zone; end
end
