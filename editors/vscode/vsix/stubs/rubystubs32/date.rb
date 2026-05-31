# frozen_string_literal: true

# \Class \Date provides methods for storing and manipulating
# calendar dates.
#
# Consider using
# {class Time}[rdoc-ref:Time]
# instead of class \Date if:
#
# - You need both dates and times; \Date handles only dates.
# - You need only Gregorian dates (and not Julian dates);
#   see {Julian and Gregorian Calendars}[rdoc-ref:calendars.rdoc].
#
# A \Date object, once created, is immutable, and cannot be modified.
#
# == Creating a \Date
#
# You can create a date for the current date, using Date.today:
#
#   Date.today # => #<Date: 1999-12-31>
#
# You can create a specific date from various combinations of arguments:
#
# - Date.new takes integer year, month, and day-of-month:
#
#     Date.new(1999, 12, 31) # => #<Date: 1999-12-31>
#
# - Date.ordinal takes integer year and day-of-year:
#
#     Date.ordinal(1999, 365) # => #<Date: 1999-12-31>
#
# - Date.jd takes integer Julian day:
#
#     Date.jd(2451544) # => #<Date: 1999-12-31>
#
# - Date.commercial takes integer commercial data (year, week, day-of-week):
#
#     Date.commercial(1999, 52, 5) # => #<Date: 1999-12-31>
#
# - Date.parse takes a string, which it parses heuristically:
#
#     Date.parse('1999-12-31')    # => #<Date: 1999-12-31>
#     Date.parse('31-12-1999')    # => #<Date: 1999-12-31>
#     Date.parse('1999-365')      # => #<Date: 1999-12-31>
#     Date.parse('1999-W52-5')    # => #<Date: 1999-12-31>
#
# - Date.strptime takes a date string and a format string,
#   then parses the date string according to the format string:
#
#     Date.strptime('1999-12-31', '%Y-%m-%d')  # => #<Date: 1999-12-31>
#     Date.strptime('31-12-1999', '%d-%m-%Y')  # => #<Date: 1999-12-31>
#     Date.strptime('1999-365', '%Y-%j')       # => #<Date: 1999-12-31>
#     Date.strptime('1999-W52-5', '%G-W%V-%u') # => #<Date: 1999-12-31>
#     Date.strptime('1999 52 5', '%Y %U %w')   # => #<Date: 1999-12-31>
#     Date.strptime('1999 52 5', '%Y %W %u')   # => #<Date: 1999-12-31>
#     Date.strptime('fri31dec99', '%a%d%b%y')  # => #<Date: 1999-12-31>
#
# See also the specialized methods in
# {"Specialized Format Strings" in Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc@Specialized+Format+Strings]
#
# == Argument +limit+
#
# Certain singleton methods in \Date that parse string arguments
# also take optional keyword argument +limit+,
# which can limit the length of the string argument.
#
# When +limit+ is:
#
# - Non-negative:
#   raises ArgumentError if the string length is greater than _limit_.
# - Other numeric or +nil+: ignores +limit+.
# - Other non-numeric: raises TypeError.
class Date
  include Comparable

  # An array of strings of abbreviated day names in English.  The
  # first is "Sun".
  ABBR_DAYNAMES = _
  # An array of strings of abbreviated month names in English.  The
  # first element is nil.
  ABBR_MONTHNAMES = _
  # An array of strings of the full names of days of the week in English.
  # The first is "Sunday".
  DAYNAMES = _
  # The Julian day number of the day of calendar reform for England
  # and her colonies.
  ENGLAND = _
  # The Julian day number of the day of calendar reform for the
  # proleptic Gregorian calendar.
  GREGORIAN = _
  # The Julian day number of the day of calendar reform for Italy
  # and some catholic countries.
  ITALY = _
  # The Julian day number of the day of calendar reform for the
  # proleptic Julian calendar.
  JULIAN = _
  # An array of strings of full month names in English.  The first
  # element is nil.
  MONTHNAMES = _

  # Returns a hash of values parsed from +string+, which should be a valid
  # {HTTP date format}[rdoc-ref:strftime_formatting.rdoc@HTTP+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.httpdate # => "Sat, 03 Feb 2001 00:00:00 GMT"
  #   Date._httpdate(s)
  #   # => {:wday=>6, :mday=>3, :mon=>2, :year=>2001, :hour=>0, :min=>0, :sec=>0, :zone=>"GMT", :offset=>0}
  #
  # Related: Date.httpdate (returns a \Date object).
  def self._httpdate(string, limit: 128) end

  # Returns a hash of values parsed from +string+, which should contain
  # an {ISO 8601 formatted date}[rdoc-ref:strftime_formatting.rdoc@ISO+8601+Format+Specifications]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.iso8601    # => "2001-02-03"
  #   Date._iso8601(s) # => {:mday=>3, :year=>2001, :mon=>2}
  #
  # See argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date.iso8601 (returns a \Date object).
  def self._iso8601(string, limit: 128) end

  # Returns a hash of values parsed from +string+, which should be a valid
  # {JIS X 0301 date format}[rdoc-ref:strftime_formatting.rdoc@JIS+X+0301+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.jisx0301    # => "H13.02.03"
  #   Date._jisx0301(s) # => {:year=>2001, :mon=>2, :mday=>3}
  #
  # See argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date.jisx0301 (returns a \Date object).
  def self._jisx0301(string, limit: 128) end

  # <b>Note</b>:
  # This method recognizes many forms in +string+,
  # but it is not a validator.
  # For formats, see
  # {"Specialized Format Strings" in Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc@Specialized+Format+Strings]
  #
  # If +string+ does not specify a valid date,
  # the result is unpredictable;
  # consider using Date._strptime instead.
  #
  # Returns a hash of values parsed from +string+:
  #
  #   Date._parse('2001-02-03') # => {:year=>2001, :mon=>2, :mday=>3}
  #
  # If +comp+ is +true+ and the given year is in the range <tt>(0..99)</tt>,
  # the current century is supplied;
  # otherwise, the year is taken as given:
  #
  #   Date._parse('01-02-03', true)  # => {:year=>2001, :mon=>2, :mday=>3}
  #   Date._parse('01-02-03', false) # => {:year=>1, :mon=>2, :mday=>3}
  #
  # See argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date.parse(returns a \Date object).
  def self._parse(string, comp = true, limit: 128) end

  # Returns a hash of values parsed from +string+, which should be a valid
  # {RFC 2822 date format}[rdoc-ref:strftime_formatting.rdoc@RFC+2822+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.rfc2822 # => "Sat, 3 Feb 2001 00:00:00 +0000"
  #   Date._rfc2822(s)
  #   # => {:wday=>6, :mday=>3, :mon=>2, :year=>2001, :hour=>0, :min=>0, :sec=>0, :zone=>"+0000", :offset=>0}
  #
  # See argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Date._rfc822 is an alias for Date._rfc2822.
  #
  # Related: Date.rfc2822 (returns a \Date object).
  def self._rfc2822(string, limit: 128) end

  # Returns a hash of values parsed from +string+, which should be a valid
  # {RFC 3339 format}[rdoc-ref:strftime_formatting.rdoc@RFC+3339+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.rfc3339     # => "2001-02-03T00:00:00+00:00"
  #   Date._rfc3339(s)
  #   # => {:year=>2001, :mon=>2, :mday=>3, :hour=>0, :min=>0, :sec=>0, :zone=>"+00:00", :offset=>0}
  #
  # See argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date.rfc3339 (returns a \Date object).
  def self._rfc3339(string, limit: 128) end

  # Returns a hash of values parsed from +string+, which should be a valid
  # {RFC 2822 date format}[rdoc-ref:strftime_formatting.rdoc@RFC+2822+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.rfc2822 # => "Sat, 3 Feb 2001 00:00:00 +0000"
  #   Date._rfc2822(s)
  #   # => {:wday=>6, :mday=>3, :mon=>2, :year=>2001, :hour=>0, :min=>0, :sec=>0, :zone=>"+0000", :offset=>0}
  #
  # See argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Date._rfc822 is an alias for Date._rfc2822.
  #
  # Related: Date.rfc2822 (returns a \Date object).
  def self._rfc822(p1, p2 = {}) end

  # Returns a hash of values parsed from +string+
  # according to the given +format+:
  #
  #   Date._strptime('2001-02-03', '%Y-%m-%d') # => {:year=>2001, :mon=>2, :mday=>3}
  #
  # For other formats, see
  # {Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc].
  # (Unlike Date.strftime, does not support flags and width.)
  #
  # See also {strptime(3)}[https://man7.org/linux/man-pages/man3/strptime.3.html].
  #
  # Related: Date.strptime (returns a \Date object).
  def self._strptime(string, format = '%F') end

  # Returns a hash of values parsed from +string+, which should be a valid
  # XML date format:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.xmlschema    # => "2001-02-03"
  #   Date._xmlschema(s) # => {:year=>2001, :mon=>2, :mday=>3}
  #
  # See argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date.xmlschema (returns a \Date object).
  def self._xmlschema(string, limit: 128) end

  # Same as Date.new.
  def self.civil(*args) end

  # Returns a new \Date object constructed from the arguments.
  #
  # Argument +cwyear+ gives the year, and should be an integer.
  #
  # Argument +cweek+ gives the index of the week within the year,
  # and should be in range (1..53) or (-53..-1);
  # in some years, 53 or -53 will be out-of-range;
  # if negative, counts backward from the end of the year:
  #
  #   Date.commercial(2022, 1, 1).to_s  # => "2022-01-03"
  #   Date.commercial(2022, 52, 1).to_s # => "2022-12-26"
  #
  # Argument +cwday+ gives the indes of the weekday within the week,
  # and should be in range (1..7) or (-7..-1);
  # 1 or -7 is Monday;
  # if negative, counts backward from the end of the week:
  #
  #   Date.commercial(2022, 1, 1).to_s  # => "2022-01-03"
  #   Date.commercial(2022, 1, -7).to_s # => "2022-01-03"
  #
  # When +cweek+ is 1:
  #
  # - If January 1 is a Friday, Saturday, or Sunday,
  #   the first week begins in the week after:
  #
  #     Date::ABBR_DAYNAMES[Date.new(2023, 1, 1).wday] # => "Sun"
  #     Date.commercial(2023, 1, 1).to_s # => "2023-01-02"
  #     Date.commercial(2023, 1, 7).to_s # => "2023-01-08"
  #
  # - Otherwise, the first week is the week of January 1,
  #   which may mean some of the days fall on the year before:
  #
  #     Date::ABBR_DAYNAMES[Date.new(2020, 1, 1).wday] # => "Wed"
  #     Date.commercial(2020, 1, 1).to_s # => "2019-12-30"
  #     Date.commercial(2020, 1, 7).to_s # => "2020-01-05"
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Related: Date.jd, Date.new, Date.ordinal.
  def self.commercial(cwyear = -4712, cweek = 1, cwday = 1, start = Date::ITALY) end

  # Returns +true+ if the given year is a leap year
  # in the {proleptic Gregorian calendar}[https://en.wikipedia.org/wiki/Proleptic_Gregorian_calendar], +false+ otherwise:
  #
  #   Date.gregorian_leap?(2000) # => true
  #   Date.gregorian_leap?(2001) # => false
  #
  # Date.leap? is an alias for Date.gregorian_leap?.
  #
  # Related: Date.julian_leap?.
  def self.gregorian_leap?(year) end

  # Returns a new \Date object with values parsed from +string+,
  # which should be a valid
  # {HTTP date format}[rdoc-ref:strftime_formatting.rdoc@HTTP+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.httpdate   # => "Sat, 03 Feb 2001 00:00:00 GMT"
  #   Date.httpdate(s) # => #<Date: 2001-02-03>
  #
  # See:
  #
  # - Argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  # - Argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date._httpdate (returns a hash).
  def self.httpdate(string = 'Mon,01 Jan -4712 00:00:00 GMT', start = Date::ITALY, limit: 128) end

  # Returns a new \Date object with values parsed from +string+,
  # which should contain
  # an {ISO 8601 formatted date}[rdoc-ref:strftime_formatting.rdoc@ISO+8601+Format+Specifications]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.iso8601   # => "2001-02-03"
  #   Date.iso8601(s) # => #<Date: 2001-02-03>
  #
  # See:
  #
  # - Argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  # - Argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date._iso8601 (returns a hash).
  def self.iso8601(string = '-4712-01-01', start = Date::ITALY, limit: 128) end

  # Returns a new \Date object formed from the arguments:
  #
  #   Date.jd(2451944).to_s # => "2001-02-03"
  #   Date.jd(2451945).to_s # => "2001-02-04"
  #   Date.jd(0).to_s       # => "-4712-01-01"
  #
  # The returned date is:
  #
  # - Gregorian, if the argument is greater than or equal to +start+:
  #
  #     Date::ITALY                         # => 2299161
  #     Date.jd(Date::ITALY).gregorian?     # => true
  #     Date.jd(Date::ITALY + 1).gregorian? # => true
  #
  # - Julian, otherwise
  #
  #     Date.jd(Date::ITALY - 1).julian?    # => true
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Related: Date.new.
  def self.jd(jd = 0, start = Date::ITALY) end

  # Returns a new \Date object with values parsed from +string+,
  # which should be a valid {JIS X 0301 format}[rdoc-ref:strftime_formatting.rdoc@JIS+X+0301+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.jisx0301   # => "H13.02.03"
  #   Date.jisx0301(s) # => #<Date: 2001-02-03>
  #
  # For no-era year, legacy format, Heisei is assumed.
  #
  #   Date.jisx0301('13.02.03') # => #<Date: 2001-02-03>
  #
  # See:
  #
  # - Argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  # - Argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date._jisx0301 (returns a hash).
  def self.jisx0301(string = '-4712-01-01', start = Date::ITALY, limit: 128) end

  # Returns +true+ if the given year is a leap year
  # in the {proleptic Julian calendar}[https://en.wikipedia.org/wiki/Proleptic_Julian_calendar], +false+ otherwise:
  #
  #   Date.julian_leap?(1900) # => true
  #   Date.julian_leap?(1901) # => false
  #
  # Related: Date.gregorian_leap?.
  def self.julian_leap?(year) end

  # Returns +true+ if the given year is a leap year
  # in the {proleptic Gregorian calendar}[https://en.wikipedia.org/wiki/Proleptic_Gregorian_calendar], +false+ otherwise:
  #
  #   Date.gregorian_leap?(2000) # => true
  #   Date.gregorian_leap?(2001) # => false
  #
  # Date.leap? is an alias for Date.gregorian_leap?.
  #
  # Related: Date.julian_leap?.
  def self.leap?(p1) end

  # Returns a new \Date object formed fom the arguments.
  #
  # With no arguments, returns the date for January 1, -4712:
  #
  #   Date.ordinal.to_s # => "-4712-01-01"
  #
  # With argument +year+, returns the date for January 1 of that year:
  #
  #   Date.ordinal(2001).to_s  # => "2001-01-01"
  #   Date.ordinal(-2001).to_s # => "-2001-01-01"
  #
  # With positive argument +yday+ == +n+,
  # returns the date for the +nth+ day of the given year:
  #
  #   Date.ordinal(2001, 14).to_s # => "2001-01-14"
  #
  # With negative argument +yday+, counts backward from the end of the year:
  #
  #   Date.ordinal(2001, -14).to_s # => "2001-12-18"
  #
  # Raises an exception if +yday+ is zero or out of range.
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Related: Date.jd, Date.new.
  def self.ordinal(year = -4712, yday = 1, start = Date::ITALY) end

  # <b>Note</b>:
  # This method recognizes many forms in +string+,
  # but it is not a validator.
  # For formats, see
  # {"Specialized Format Strings" in Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc@Specialized+Format+Strings]
  # If +string+ does not specify a valid date,
  # the result is unpredictable;
  # consider using Date._strptime instead.
  #
  # Returns a new \Date object with values parsed from +string+:
  #
  #   Date.parse('2001-02-03')   # => #<Date: 2001-02-03>
  #   Date.parse('20010203')     # => #<Date: 2001-02-03>
  #   Date.parse('3rd Feb 2001') # => #<Date: 2001-02-03>
  #
  # If +comp+ is +true+ and the given year is in the range <tt>(0..99)</tt>,
  # the current century is supplied;
  # otherwise, the year is taken as given:
  #
  #   Date.parse('01-02-03', true)  # => #<Date: 2001-02-03>
  #   Date.parse('01-02-03', false) # => #<Date: 0001-02-03>
  #
  # See:
  #
  # - Argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  # - Argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date._parse (returns a hash).
  def self.parse(string = '-4712-01-01', comp = true, start = Date::ITALY, limit: 128) end

  # Returns a new \Date object with values parsed from +string+,
  # which should be a valid
  # {RFC 2822 date format}[rdoc-ref:strftime_formatting.rdoc@RFC+2822+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.rfc2822   # => "Sat, 3 Feb 2001 00:00:00 +0000"
  #   Date.rfc2822(s) # => #<Date: 2001-02-03>
  #
  # See:
  #
  # - Argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  # - Argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Date.rfc822 is an alias for Date.rfc2822.
  #
  # Related: Date._rfc2822 (returns a hash).
  def self.rfc2822(string = 'Mon,1 Jan -4712 00:00:00 +0000', start = Date::ITALY, limit: 128) end

  # Returns a new \Date object with values parsed from +string+,
  # which should be a valid
  # {RFC 3339 format}[rdoc-ref:strftime_formatting.rdoc@RFC+3339+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.rfc3339   # => "2001-02-03T00:00:00+00:00"
  #   Date.rfc3339(s) # => #<Date: 2001-02-03>
  #
  # See:
  #
  # - Argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  # - Argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date._rfc3339 (returns a hash).
  def self.rfc3339(string = '-4712-01-01T00:00:00+00:00', start = Date::ITALY, limit: 128) end

  # Returns a new \Date object with values parsed from +string+,
  # which should be a valid
  # {RFC 2822 date format}[rdoc-ref:strftime_formatting.rdoc@RFC+2822+Format]:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.rfc2822   # => "Sat, 3 Feb 2001 00:00:00 +0000"
  #   Date.rfc2822(s) # => #<Date: 2001-02-03>
  #
  # See:
  #
  # - Argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  # - Argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Date.rfc822 is an alias for Date.rfc2822.
  #
  # Related: Date._rfc2822 (returns a hash).
  def self.rfc822(p1 = v1, p2 = v2, p3 = {}) end

  # Returns a new \Date object with values parsed from +string+,
  # according to the given +format+:
  #
  #   Date.strptime('2001-02-03', '%Y-%m-%d')  # => #<Date: 2001-02-03>
  #   Date.strptime('03-02-2001', '%d-%m-%Y')  # => #<Date: 2001-02-03>
  #   Date.strptime('2001-034', '%Y-%j')       # => #<Date: 2001-02-03>
  #   Date.strptime('2001-W05-6', '%G-W%V-%u') # => #<Date: 2001-02-03>
  #   Date.strptime('2001 04 6', '%Y %U %w')   # => #<Date: 2001-02-03>
  #   Date.strptime('2001 05 6', '%Y %W %u')   # => #<Date: 2001-02-03>
  #   Date.strptime('sat3feb01', '%a%d%b%y')   # => #<Date: 2001-02-03>
  #
  # For other formats, see
  # {Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc].
  # (Unlike Date.strftime, does not support flags and width.)
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # See also {strptime(3)}[https://man7.org/linux/man-pages/man3/strptime.3.html].
  #
  # Related: Date._strptime (returns a hash).
  def self.strptime(string = '-4712-01-01', format = '%F', start = Date::ITALY) end

  # Returns a new \Date object constructed from the present date:
  #
  #   Date.today.to_s # => "2022-07-06"
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  def self.today(start = Date::ITALY) end

  # Returns +true+ if the arguments define a valid ordinal date,
  # +false+ otherwise:
  #
  #   Date.valid_date?(2001, 2, 3)  # => true
  #   Date.valid_date?(2001, 2, 29) # => false
  #   Date.valid_date?(2001, 2, -1) # => true
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Date.valid_date? is an alias for Date.valid_civil?.
  #
  # Related: Date.jd, Date.new.
  def self.valid_civil?(year, month, mday, start = Date::ITALY) end

  # Returns +true+ if the arguments define a valid commercial date,
  # +false+ otherwise:
  #
  #   Date.valid_commercial?(2001, 5, 6) # => true
  #   Date.valid_commercial?(2001, 5, 8) # => false
  #
  # See Date.commercial.
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Related: Date.jd, Date.commercial.
  def self.valid_commercial?(cwyear, cweek, cwday, start = Date::ITALY) end

  # Returns +true+ if the arguments define a valid ordinal date,
  # +false+ otherwise:
  #
  #   Date.valid_date?(2001, 2, 3)  # => true
  #   Date.valid_date?(2001, 2, 29) # => false
  #   Date.valid_date?(2001, 2, -1) # => true
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Date.valid_date? is an alias for Date.valid_civil?.
  #
  # Related: Date.jd, Date.new.
  def self.valid_date?(p1, p2, p3, p4 = v4) end

  # Implemented for compatibility;
  # returns +true+ unless +jd+ is invalid (i.e., not a Numeric).
  #
  #   Date.valid_jd?(2451944) # => true
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Related: Date.jd.
  def self.valid_jd?(jd, start = Date::ITALY) end

  # Returns +true+ if the arguments define a valid ordinal date,
  # +false+ otherwise:
  #
  #   Date.valid_ordinal?(2001, 34)  # => true
  #   Date.valid_ordinal?(2001, 366) # => false
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Related: Date.jd, Date.ordinal.
  def self.valid_ordinal?(year, yday, start = Date::ITALY) end

  # Returns a new \Date object with values parsed from +string+,
  # which should be a valid XML date format:
  #
  #   d = Date.new(2001, 2, 3)
  #   s = d.xmlschema   # => "2001-02-03"
  #   Date.xmlschema(s) # => #<Date: 2001-02-03>
  #
  # See:
  #
  # - Argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  # - Argument {limit}[rdoc-ref:Date@Argument+limit].
  #
  # Related: Date._xmlschema (returns a hash).
  def self.xmlschema(string = '-4712-01-01', start = Date::ITALY, limit: 128) end

  # Returns a new \Date object constructed from the given arguments:
  #
  #   Date.new(2022).to_s        # => "2022-01-01"
  #   Date.new(2022, 2).to_s     # => "2022-02-01"
  #   Date.new(2022, 2, 4).to_s  # => "2022-02-04"
  #
  # Argument +month+ should be in range (1..12) or range (-12..-1);
  # when the argument is negative, counts backward from the end of the year:
  #
  #   Date.new(2022, -11, 4).to_s # => "2022-02-04"
  #
  # Argument +mday+ should be in range (1..n) or range (-n..-1)
  # where +n+ is the number of days in the month;
  # when the argument is negative, counts backward from the end of the month.
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  #
  # Date.civil is an alias for Date.new.
  #
  # Related: Date.jd.
  def initialize(year = -4712, month = 1, mday = 1, start = Date::ITALY) end

  # Returns a date object pointing +other+ days after self.  The other
  # should be a numeric value.  If the other is a fractional number,
  # assumes its precision is at most nanosecond.
  #
  #    Date.new(2001,2,3) + 1    #=> #<Date: 2001-02-04 ...>
  #    DateTime.new(2001,2,3) + Rational(1,2)
  #                              #=> #<DateTime: 2001-02-03T12:00:00+00:00 ...>
  #    DateTime.new(2001,2,3) + Rational(-1,2)
  #                              #=> #<DateTime: 2001-02-02T12:00:00+00:00 ...>
  #    DateTime.jd(0,12) + DateTime.new(2001,2,3).ajd
  #                              #=> #<DateTime: 2001-02-03T00:00:00+00:00 ...>
  def +(other) end

  # Returns the difference between the two dates if the other is a date
  # object.  If the other is a numeric value, returns a date object
  # pointing +other+ days before self.  If the other is a fractional number,
  # assumes its precision is at most nanosecond.
  #
  #     Date.new(2001,2,3) - 1   #=> #<Date: 2001-02-02 ...>
  #     DateTime.new(2001,2,3) - Rational(1,2)
  #                              #=> #<DateTime: 2001-02-02T12:00:00+00:00 ...>
  #     Date.new(2001,2,3) - Date.new(2001)
  #                              #=> (33/1)
  #     DateTime.new(2001,2,3) - DateTime.new(2001,2,2,12)
  #                              #=> (1/2)
  def -(other) end

  # Returns a new \Date object representing the date
  # +n+ months earlier; +n+ should be a numeric:
  #
  #   (Date.new(2001, 2, 3) << 1).to_s  # => "2001-01-03"
  #   (Date.new(2001, 2, 3) << -2).to_s # => "2001-04-03"
  #
  # When the same day does not exist for the new month,
  # the last day of that month is used instead:
  #
  #   (Date.new(2001, 3, 31) << 1).to_s  # => "2001-02-28"
  #   (Date.new(2001, 3, 31) << -6).to_s # => "2001-09-30"
  #
  # This results in the following, possibly unexpected, behaviors:
  #
  #   d0 = Date.new(2001, 3, 31)
  #   d0 << 2      # => #<Date: 2001-01-31>
  #   d0 << 1 << 1 # => #<Date: 2001-01-28>
  #
  #   d0 = Date.new(2001, 3, 31)
  #   d1 = d0 << 1  # => #<Date: 2001-02-28>
  #   d2 = d1 << -1 # => #<Date: 2001-03-28>
  def <<(n) end

  # Compares +self+ and +other+, returning:
  #
  # - <tt>-1</tt> if +other+ is larger.
  # - <tt>0</tt> if the two are equal.
  # - <tt>1</tt> if +other+ is smaller.
  # - +nil+ if the two are incomparable.
  #
  # Argument +other+ may be:
  #
  # - Another \Date object:
  #
  #     d = Date.new(2022, 7, 27) # => #<Date: 2022-07-27 ((2459788j,0s,0n),+0s,2299161j)>
  #     prev_date = d.prev_day    # => #<Date: 2022-07-26 ((2459787j,0s,0n),+0s,2299161j)>
  #     next_date = d.next_day    # => #<Date: 2022-07-28 ((2459789j,0s,0n),+0s,2299161j)>
  #     d <=> next_date           # => -1
  #     d <=> d                   # => 0
  #     d <=> prev_date           # => 1
  #
  # - A DateTime object:
  #
  #     d <=> DateTime.new(2022, 7, 26) # => 1
  #     d <=> DateTime.new(2022, 7, 27) # => 0
  #     d <=> DateTime.new(2022, 7, 28) # => -1
  #
  # - A numeric (compares <tt>self.ajd</tt> to +other+):
  #
  #     d <=> 2459788 # => -1
  #     d <=> 2459787 # => 1
  #     d <=> 2459786 # => 1
  #     d <=> d.ajd   # => 0
  #
  # - Any other object:
  #
  #     d <=> Object.new # => nil
  def <=>(other) end

  # Returns +true+ if +self+ and +other+ represent the same date,
  # +false+ if not, +nil+ if the two are not comparable.
  #
  # Argument +other+ may be:
  #
  # - Another \Date object:
  #
  #     d = Date.new(2022, 7, 27) # => #<Date: 2022-07-27 ((2459788j,0s,0n),+0s,2299161j)>
  #     prev_date = d.prev_day    # => #<Date: 2022-07-26 ((2459787j,0s,0n),+0s,2299161j)>
  #     next_date = d.next_day    # => #<Date: 2022-07-28 ((2459789j,0s,0n),+0s,2299161j)>
  #     d === prev_date           # => false
  #     d === d                   # => true
  #     d === next_date           # => false
  #
  # - A DateTime object:
  #
  #     d === DateTime.new(2022, 7, 26) # => false
  #     d === DateTime.new(2022, 7, 27) # => true
  #     d === DateTime.new(2022, 7, 28) # => false
  #
  # - A numeric (compares <tt>self.jd</tt> to +other+):
  #
  #     d === 2459788 # => true
  #     d === 2459787 # => false
  #     d === 2459786 # => false
  #     d === d.jd    # => true
  #
  # - An object not comparable:
  #
  #     d === Object.new # => nil
  def ===(other) end

  # Returns a new \Date object representing the date
  # +n+ months later; +n+ should be a numeric:
  #
  #   (Date.new(2001, 2, 3) >> 1).to_s  # => "2001-03-03"
  #   (Date.new(2001, 2, 3) >> -2).to_s # => "2000-12-03"
  #
  # When the same day does not exist for the new month,
  # the last day of that month is used instead:
  #
  #   (Date.new(2001, 1, 31) >> 1).to_s  # => "2001-02-28"
  #   (Date.new(2001, 1, 31) >> -4).to_s # => "2000-09-30"
  #
  # This results in the following, possibly unexpected, behaviors:
  #
  #   d0 = Date.new(2001, 1, 31)
  #   d1 = d0 >> 1 # => #<Date: 2001-02-28>
  #   d2 = d1 >> 1 # => #<Date: 2001-03-28>
  #
  #   d0 = Date.new(2001, 1, 31)
  #   d1 = d0 >> 1  # => #<Date: 2001-02-28>
  #   d2 = d1 >> -1 # => #<Date: 2001-01-28>
  def >>(other) end

  # Returns the astronomical Julian day number.  This is a fractional
  # number, which is not adjusted by the offset.
  #
  #    DateTime.new(2001,2,3,4,5,6,'+7').ajd     #=> (11769328217/4800)
  #    DateTime.new(2001,2,2,14,5,6,'-7').ajd    #=> (11769328217/4800)
  def ajd; end

  # Returns the astronomical modified Julian day number.  This is
  # a fractional number, which is not adjusted by the offset.
  #
  #    DateTime.new(2001,2,3,4,5,6,'+7').amjd    #=> (249325817/4800)
  #    DateTime.new(2001,2,2,14,5,6,'-7').amjd   #=> (249325817/4800)
  def amjd; end

  # Equivalent to #strftime with argument <tt>'%a %b %e %T %Y'</tt>
  # (or its {shorthand form}[rdoc-ref:strftime_formatting.rdoc@Shorthand+Conversion+Specifiers]
  # <tt>'%c'</tt>):
  #
  #   Date.new(2001, 2, 3).asctime # => "Sat Feb  3 00:00:00 2001"
  #
  # See {asctime}[https://linux.die.net/man/3/asctime].
  #
  # Date#ctime is an alias for Date#asctime.
  def asctime; end
  alias ctime asctime

  # Returns the commercial-date weekday index for +self+
  # (see Date.commercial);
  # 1 is Monday:
  #
  #   Date.new(2001, 2, 3).cwday # => 6
  def cwday; end

  # Returns commercial-date week index for +self+
  # (see Date.commercial):
  #
  #   Date.new(2001, 2, 3).cweek # => 5
  def cweek; end

  # Returns commercial-date year for +self+
  # (see Date.commercial):
  #
  #   Date.new(2001, 2, 3).cwyear # => 2001
  #   Date.new(2000, 1, 1).cwyear # => 1999
  def cwyear; end

  # Returns the fractional part of the day in range (Rational(0, 1)...Rational(1, 1)):
  #
  #   DateTime.new(2001,2,3,12).day_fraction # => (1/2)
  def day_fraction; end

  # Returns a hash of the name/value pairs, to use in pattern matching.
  # Possible keys are: <tt>:year</tt>, <tt>:month</tt>, <tt>:day</tt>,
  # <tt>:wday</tt>, <tt>:yday</tt>.
  #
  # Possible usages:
  #
  #   d = Date.new(2022, 10, 5)
  #
  #   if d in wday: 3, day: ..7  # uses deconstruct_keys underneath
  #     puts "first Wednesday of the month"
  #   end
  #   #=> prints "first Wednesday of the month"
  #
  #   case d
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
  #   if d in Date(wday: 3, day: ..7)
  #     puts "first Wednesday of the month"
  #   end
  def deconstruct_keys(array_of_names_or_nil) end

  # Equivalent to #step with arguments +min+ and <tt>-1</tt>.
  def downto(min) end

  # Equivalent to Date#new_start with argument Date::ENGLAND.
  def england; end

  # Returns +true+ if +self+ is a Friday, +false+ otherwise.
  def friday?; end

  # Equivalent to Date#new_start with argument Date::GREGORIAN.
  def gregorian; end

  # Returns +true+ if the date is on or after
  # the date of calendar reform, +false+ otherwise:
  #
  #   Date.new(1582, 10, 15).gregorian?       # => true
  #   (Date.new(1582, 10, 15) - 1).gregorian? # => false
  def gregorian?; end

  # Equivalent to #strftime with argument <tt>'%a, %d %b %Y %T GMT'</tt>;
  # see {Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc]:
  #
  #   Date.new(2001, 2, 3).httpdate # => "Sat, 03 Feb 2001 00:00:00 GMT"
  def httpdate; end

  # Returns a string representation of +self+:
  #
  #   Date.new(2001, 2, 3).inspect
  #   # => "#<Date: 2001-02-03 ((2451944j,0s,0n),+0s,2299161j)>"
  def inspect; end

  # Equivalent to #strftime with argument <tt>'%Y-%m-%d'</tt>
  # (or its {shorthand form}[rdoc-ref:strftime_formatting.rdoc@Shorthand+Conversion+Specifiers]
  # <tt>'%F'</tt>);
  #
  #   Date.new(2001, 2, 3).iso8601 # => "2001-02-03"
  #
  # Date#xmlschema is an alias for Date#iso8601.
  def iso8601; end
  alias xmlschema iso8601

  # Equivalent to Date#new_start with argument Date::ITALY.
  def italy; end

  # Returns the Julian day number.  This is a whole number, which is
  # adjusted by the offset as the local time.
  #
  #    DateTime.new(2001,2,3,4,5,6,'+7').jd      #=> 2451944
  #    DateTime.new(2001,2,3,4,5,6,'-7').jd      #=> 2451944
  def jd; end

  # Returns a string representation of the date in +self+
  # in JIS X 0301 format.
  #
  #   Date.new(2001, 2, 3).jisx0301 # => "H13.02.03"
  def jisx0301; end

  # Equivalent to Date#new_start with argument Date::JULIAN.
  def julian; end

  # Returns +true+ if the date is before the date of calendar reform,
  # +false+ otherwise:
  #
  #   (Date.new(1582, 10, 15) - 1).julian? # => true
  #   Date.new(1582, 10, 15).julian?       # => false
  def julian?; end

  # Returns the
  # {Lilian day number}[https://en.wikipedia.org/wiki/Lilian_date],
  # which is the number of days since the beginning of the Gregorian
  # calendar, October 15, 1582.
  #
  #   Date.new(2001, 2, 3).ld # => 152784
  def ld; end

  # Returns +true+ if the year is a leap year, +false+ otherwise:
  #
  #   Date.new(2000).leap? # => true
  #   Date.new(2001).leap? # => false
  def leap?; end

  # Returns the day of the month in range (1..31):
  #
  #   Date.new(2001, 2, 3).mday # => 3
  #
  # Date#day is an alias for Date#mday.
  def mday; end
  alias day mday

  # Returns the modified Julian day number.  This is a whole number,
  # which is adjusted by the offset as the local time.
  #
  #    DateTime.new(2001,2,3,4,5,6,'+7').mjd     #=> 51943
  #    DateTime.new(2001,2,3,4,5,6,'-7').mjd     #=> 51943
  def mjd; end

  # Returns the month in range (1..12):
  #
  #   Date.new(2001, 2, 3).mon # => 2
  #
  # Date#month is an alias for Date#mon.
  def mon; end
  alias month mon

  # Returns +true+ if +self+ is a Monday, +false+ otherwise.
  def monday?; end

  # Returns a copy of +self+ with the given +start+ value:
  #
  #   d0 = Date.new(2000, 2, 3)
  #   d0.julian? # => false
  #   d1 = d0.new_start(Date::JULIAN)
  #   d1.julian? # => true
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  def new_start(p1 = v1) end

  # Returns a new \Date object representing the following day:
  #
  #   d = Date.new(2001, 2, 3)
  #   d.to_s      # => "2001-02-03"
  #   d.next.to_s # => "2001-02-04"
  #
  # Date#succ is an alias for Date#next.
  def next; end
  alias succ next

  # Equivalent to Date#+ with argument +n+.
  def next_day(n = 1) end

  # Equivalent to #>> with argument +n+.
  def next_month(n = 1) end

  # Equivalent to #>> with argument <tt>n * 12</tt>.
  def next_year(n = 1) end

  # Equivalent to Date#- with argument +n+.
  def prev_day(n = 1) end

  # Equivalent to #<< with argument +n+.
  def prev_month(n = 1) end

  # Equivalent to #<< with argument <tt>n * 12</tt>.
  def prev_year(n = 1) end

  # Equivalent to #strftime with argument <tt>'%a, %-d %b %Y %T %z'</tt>;
  # see {Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc]:
  #
  #   Date.new(2001, 2, 3).rfc2822 # => "Sat, 3 Feb 2001 00:00:00 +0000"
  #
  # Date#rfc822 is an alias for Date#rfc2822.
  def rfc2822; end
  alias rfc822 rfc2822

  # Equivalent to #strftime with argument <tt>'%FT%T%:z'</tt>;
  # see {Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc]:
  #
  #   Date.new(2001, 2, 3).rfc3339 # => "2001-02-03T00:00:00+00:00"
  def rfc3339; end

  # Returns +true+ if +self+ is a Saturday, +false+ otherwise.
  def saturday?; end

  # Returns the Julian start date for calendar reform;
  # if not an infinity, the returned value is suitable
  # for passing to Date#jd:
  #
  #   d = Date.new(2001, 2, 3, Date::ITALY)
  #   s = d.start     # => 2299161.0
  #   Date.jd(s).to_s # => "1582-10-15"
  #
  #   d = Date.new(2001, 2, 3, Date::ENGLAND)
  #   s = d.start     # => 2361222.0
  #   Date.jd(s).to_s # => "1752-09-14"
  #
  #   Date.new(2001, 2, 3, Date::GREGORIAN).start # => -Infinity
  #   Date.new(2001, 2, 3, Date::JULIAN).start    # => Infinity
  #
  # See argument {start}[rdoc-ref:calendars.rdoc@Argument+start].
  def start; end

  # Calls the block with specified dates;
  # returns +self+.
  #
  # - The first +date+ is +self+.
  # - Each successive +date+ is <tt>date + step</tt>,
  #   where +step+ is the numeric step size in days.
  # - The last date is the last one that is before or equal to +limit+,
  #   which should be a \Date object.
  #
  # Example:
  #
  #   limit = Date.new(2001, 12, 31)
  #   Date.new(2001).step(limit){|date| p date.to_s if date.mday == 31 }
  #
  # Output:
  #
  #   "2001-01-31"
  #   "2001-03-31"
  #   "2001-05-31"
  #   "2001-07-31"
  #   "2001-08-31"
  #   "2001-10-31"
  #   "2001-12-31"
  #
  # Returns an Enumerator if no block is given.
  def step(limit, step = 1) end

  # Returns a string representation of the date in +self+,
  # formatted according the given +format+:
  #
  #   Date.new(2001, 2, 3).strftime # => "2001-02-03"
  #
  # For other formats, see
  # {Formats for Dates and Times}[rdoc-ref:strftime_formatting.rdoc].
  def strftime(format = '%F') end

  # Returns +true+ if +self+ is a Sunday, +false+ otherwise.
  def sunday?; end

  # Returns +true+ if +self+ is a Thursday, +false+ otherwise.
  def thursday?; end

  # Returns +self+.
  def to_date; end

  # Returns a DateTime whose value is the same as +self+:
  #
  #   Date.new(2001, 2, 3).to_datetime # => #<DateTime: 2001-02-03T00:00:00+00:00>
  def to_datetime; end

  # Returns a string representation of the date in +self+
  # in {ISO 8601 extended date format}[rdoc-ref:strftime_formatting.rdoc@ISO+8601+Format+Specifications]
  # (<tt>'%Y-%m-%d'</tt>):
  #
  #   Date.new(2001, 2, 3).to_s # => "2001-02-03"
  def to_s; end

  # Returns a new Time object with the same value as +self+;
  # if +self+ is a Julian date, derives its Gregorian date
  # for conversion to the \Time object:
  #
  #   Date.new(2001, 2, 3).to_time               # => 2001-02-03 00:00:00 -0600
  #   Date.new(2001, 2, 3, Date::JULIAN).to_time # => 2001-02-16 00:00:00 -0600
  def to_time; end

  # Returns +true+ if +self+ is a Tuesday, +false+ otherwise.
  def tuesday?; end

  # Equivalent to #step with arguments +max+ and +1+.
  def upto(max) end

  # Returns the day of week in range (0..6); Sunday is 0:
  #
  #   Date.new(2001, 2, 3).wday # => 6
  def wday; end

  # Returns +true+ if +self+ is a Wednesday, +false+ otherwise.
  def wednesday?; end

  # Returns the day of the year, in range (1..366):
  #
  #   Date.new(2001, 2, 3).yday # => 34
  def yday; end

  # Returns the year:
  #
  #   Date.new(2001, 2, 3).year    # => 2001
  #   (Date.new(1, 1, 1) - 1).year # => 0
  def year; end

  # Exception for invalid date/time
  class Error < ArgumentError
  end
end
