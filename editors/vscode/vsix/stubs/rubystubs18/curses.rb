# frozen_string_literal: true

# ------------------------- Initialization -------------------------
module Curses
  ALL_MOUSE_EVENTS = _
  A_ALTCHARSET = _
  A_ATTRIBUTES = _
  A_BLINK = _
  A_BOLD = _
  A_CHARTEXT = _
  A_COLOR = _
  A_DIM = _
  A_HORIZONTAL = _
  A_INVIS = _
  A_LEFT = _
  A_LOW = _
  A_NORMAL = _
  A_PROTECT = _
  A_REVERSE = _
  A_RIGHT = _
  A_STANDOUT = _
  A_TOP = _
  A_UNDERLINE = _
  A_VERTICAL = _
  BUTTON1_CLICKED = _
  BUTTON1_DOUBLE_CLICKED = _
  BUTTON1_PRESSED = _
  BUTTON1_RELEASED = _
  BUTTON1_TRIPLE_CLICKED = _
  BUTTON2_CLICKED = _
  BUTTON2_DOUBLE_CLICKED = _
  BUTTON2_PRESSED = _
  BUTTON2_RELEASED = _
  BUTTON2_TRIPLE_CLICKED = _
  BUTTON3_CLICKED = _
  BUTTON3_DOUBLE_CLICKED = _
  BUTTON3_PRESSED = _
  BUTTON3_RELEASED = _
  BUTTON3_TRIPLE_CLICKED = _
  BUTTON4_CLICKED = _
  BUTTON4_DOUBLE_CLICKED = _
  BUTTON4_PRESSED = _
  BUTTON4_RELEASED = _
  BUTTON4_TRIPLE_CLICKED = _
  BUTTON_ALT = _
  BUTTON_CTRL = _
  BUTTON_SHIFT = _
  COLORS = _
  COLOR_BLACK = _
  COLOR_BLUE = _
  COLOR_CYAN = _
  COLOR_GREEN = _
  COLOR_MAGENTA = _
  COLOR_RED = _
  COLOR_WHITE = _
  COLOR_YELLOW = _
  KEY_A1 = _
  KEY_A3 = _
  KEY_B2 = _
  KEY_BACKSPACE = _
  KEY_BEG = _
  KEY_BREAK = _
  KEY_BTAB = _
  KEY_C1 = _
  KEY_C3 = _
  KEY_CANCEL = _
  KEY_CATAB = _
  KEY_CLEAR = _
  KEY_CLOSE = _
  KEY_COMMAND = _
  KEY_COPY = _
  KEY_CREATE = _
  KEY_CTAB = _
  KEY_DC = _
  KEY_DL = _
  KEY_DOWN = _
  KEY_EIC = _
  KEY_END = _
  KEY_ENTER = _
  KEY_EOL = _
  KEY_EOS = _
  KEY_EXIT = _
  KEY_FIND = _
  KEY_HELP = _
  KEY_HOME = _
  KEY_IC = _
  KEY_IL = _
  KEY_LEFT = _
  KEY_LL = _
  KEY_MARK = _
  KEY_MAX = _
  KEY_MESSAGE = _
  KEY_MIN = _
  KEY_MOUSE = _
  KEY_MOVE = _
  KEY_NEXT = _
  KEY_NPAGE = _
  KEY_OPEN = _
  KEY_OPTIONS = _
  KEY_PPAGE = _
  KEY_PREVIOUS = _
  KEY_PRINT = _
  KEY_REDO = _
  KEY_REFERENCE = _
  KEY_REFRESH = _
  KEY_REPLACE = _
  KEY_RESET = _
  KEY_RESIZE = _
  KEY_RESTART = _
  KEY_RESUME = _
  KEY_RIGHT = _
  KEY_SAVE = _
  KEY_SBEG = _
  KEY_SCANCEL = _
  KEY_SCOMMAND = _
  KEY_SCOPY = _
  KEY_SCREATE = _
  KEY_SDC = _
  KEY_SDL = _
  KEY_SELECT = _
  KEY_SEND = _
  KEY_SEOL = _
  KEY_SEXIT = _
  KEY_SF = _
  KEY_SFIND = _
  KEY_SHELP = _
  KEY_SHOME = _
  KEY_SIC = _
  KEY_SLEFT = _
  KEY_SMESSAGE = _
  KEY_SMOVE = _
  KEY_SNEXT = _
  KEY_SOPTIONS = _
  KEY_SPREVIOUS = _
  KEY_SPRINT = _
  KEY_SR = _
  KEY_SREDO = _
  KEY_SREPLACE = _
  KEY_SRESET = _
  KEY_SRIGHT = _
  KEY_SRSUME = _
  KEY_SSAVE = _
  KEY_SSUSPEND = _
  KEY_STAB = _
  KEY_SUNDO = _
  KEY_SUSPEND = _
  KEY_UNDO = _
  KEY_UP = _
  REPORT_MOUSE_POSITION = _

  # def addch(ch)
  def self.addch(p1) end

  # def addstr(str)
  def self.addstr(p1) end

  def self.attroff(p1) end

  def self.attron(p1) end

  def self.attrset(p1) end

  # def beep
  def self.beep; end

  def self.bkgd(p1) end

  def self.bkgdset(p1) end

  def self.can_change_color?; end

  # def cbreak
  def self.cbreak; end

  # def clear
  def self.clear; end

  # def close_screen
  def self.close_screen; end

  # def closed?
  def self.closed?; end

  # def clrtoeol
  def self.clrtoeol; end

  def self.color_content(p1) end

  def self.color_pair(p1) end

  def self.cols; end

  def self.curs_set(p1) end

  def self.def_prog_mode; end

  # def delch
  def self.delch; end

  # def delelteln
  def self.deleteln; end

  # def doupdate
  def self.doupdate; end

  # def echo
  def self.echo; end

  # def flash
  def self.flash; end

  # def getch
  def self.getch; end

  def self.getmouse; end

  # def getstr
  def self.getstr; end

  def self.has_colors?; end

  # def inch
  def self.inch; end

  def self.init_color(p1, p2, p3, p4) end

  def self.init_pair(p1, p2, p3) end

  # def init_screen
  def self.init_screen; end

  # def insch(ch)
  def self.insch(p1) end

  # def insertln
  def self.insertln; end

  # def keyname
  def self.keyname(p1) end

  def self.lines; end

  def self.mouseinterval(p1) end

  def self.mousemask(p1) end

  # def nl
  def self.nl; end

  # def nocbreak
  def self.nocbreak; end

  # def noecho
  def self.noecho; end

  # def nonl
  def self.nonl; end

  # def noraw
  def self.noraw; end

  def self.pair_content(p1) end

  def self.pair_number(p1) end

  # def raw
  def self.raw; end

  # def refresh
  def self.refresh; end

  def self.reset_prog_mode; end

  def self.resize(p1, p2) end

  def self.resizeterm(p1, p2) end

  def self.scrl(p1) end

  # def setpos(y, x)
  def self.setpos(p1, p2) end

  def self.setscrreg(p1, p2) end

  # def standend
  def self.standend; end

  # def standout
  def self.standout; end

  def self.start_color; end

  # def stdscr
  def self.stdscr; end

  # USE_MOUSE
  def self.timeout=(p1) end

  # def ungetch
  def self.ungetch(p1) end

  def self.ungetmouse(p1) end

  private

  # def addch(ch)
  def addch(p1) end

  # def addstr(str)
  def addstr(p1) end

  def attroff(p1) end

  def attron(p1) end

  def attrset(p1) end

  # def beep
  def beep; end

  def bkgd(p1) end

  def bkgdset(p1) end

  def can_change_color?; end

  # def cbreak
  def cbreak; end
  alias crmode cbreak

  # def clear
  def clear; end

  # def close_screen
  def close_screen; end

  # def closed?
  def closed?; end

  # def clrtoeol
  def clrtoeol; end

  def color_content(p1) end

  def color_pair(p1) end

  def cols; end

  def curs_set(p1) end

  def def_prog_mode; end

  # def delch
  def delch; end

  # def delelteln
  def deleteln; end

  # def doupdate
  def doupdate; end

  # def echo
  def echo; end

  # def flash
  def flash; end

  # def getch
  def getch; end

  def getmouse; end

  # def getstr
  def getstr; end

  def has_colors?; end

  # def inch
  def inch; end

  def init_color(p1, p2, p3, p4) end

  def init_pair(p1, p2, p3) end

  # def init_screen
  def init_screen; end

  # def insch(ch)
  def insch(p1) end

  # def insertln
  def insertln; end

  # def keyname
  def keyname(p1) end

  def lines; end

  def mouseinterval(p1) end

  def mousemask(p1) end

  # def nl
  def nl; end

  # def nocbreak
  def nocbreak; end
  alias nocrmode nocbreak

  # def noecho
  def noecho; end

  # def nonl
  def nonl; end

  # def noraw
  def noraw; end

  def pair_content(p1) end

  def pair_number(p1) end

  # def raw
  def raw; end

  # def refresh
  def refresh; end

  def reset_prog_mode; end

  def resizeterm(p1, p2) end
  alias resize resizeterm

  def scrl(p1) end

  # def setpos(y, x)
  def setpos(p1, p2) end

  def setscrreg(p1, p2) end

  # def standend
  def standend; end

  # def standout
  def standout; end

  def start_color; end

  # def stdscr
  def stdscr; end

  # USE_MOUSE
  def timeout=(p1) end

  # def ungetch
  def ungetch(p1) end

  def ungetmouse(p1) end

  module Key
    A1 = _
    A3 = _
    B2 = _
    BACKSPACE = _
    BEG = _
    BREAK = _
    BTAB = _
    C1 = _
    C3 = _
    CANCEL = _
    CATAB = _
    CLEAR = _
    CLOSE = _
    COMMAND = _
    COPY = _
    CREATE = _
    CTAB = _
    DC = _
    DL = _
    DOWN = _
    EIC = _
    ENTER = _
    EOL = _
    EOS = _
    EXIT = _
    FIND = _
    HELP = _
    HOME = _
    IC = _
    IL = _
    LEFT = _
    LL = _
    MARK = _
    MAX = _
    MESSAGE = _
    MIN = _
    MOUSE = _
    MOVE = _
    NEXT = _
    NPAGE = _
    OPEN = _
    OPTIONS = _
    PPAGE = _
    PREVIOUS = _
    PRINT = _
    REDO = _
    REFERENCE = _
    REFRESH = _
    REPLACE = _
    RESET = _
    RESIZE = _
    RESTART = _
    RESUME = _
    RIGHT = _
    SAVE = _
    SBEG = _
    SCANCEL = _
    SCOMMAND = _
    SCOPY = _
    SCREATE = _
    SDC = _
    SDL = _
    SELECT = _
    SEND = _
    SEOL = _
    SEXIT = _
    SF = _
    SFIND = _
    SHELP = _
    SHOME = _
    SIC = _
    SLEFT = _
    SMESSAGE = _
    SMOVE = _
    SNEXT = _
    SOPTIONS = _
    SPREVIOUS = _
    SPRINT = _
    SR = _
    SREDO = _
    SREPLACE = _
    SRESET = _
    SRIGHT = _
    SRSUME = _
    SSAVE = _
    SSUSPEND = _
    STAB = _
    SUNDO = _
    SUSPEND = _
    UNDO = _
    UP = _
  end

  class MouseEvent
  end

  class Window < Data
    # def initialize(h, w, top, left)
    def initialize(p1, p2, p3, p4) end

    # def <<(str)
    def <<(p1) end

    # def addch(ch)
    def addch(p1) end

    # def addstr(str)
    def addstr(p1) end

    def attroff(p1) end

    def attron(p1) end

    def attrset(p1) end

    # def begx
    def begx; end

    # def begy
    def begy; end

    def bkgd(p1) end

    def bkgdset(p1) end

    # def box(vert, hor)
    def box(p1, p2, p3 = v3) end

    # def clear
    def clear; end

    # def close
    def close; end

    # def clrtoeol
    def clrtoeol; end

    def color_set(p1) end

    # def curx
    def curx; end

    # def cury
    def cury; end

    # def delch
    def delch; end

    # def delelteln
    def deleteln; end

    def getbkgd; end

    # def getch
    def getch; end

    # def getstr
    def getstr; end

    def idlok(p1) end

    # def inch
    def inch; end

    # def insch(ch)
    def insch(p1) end

    # def insertln
    def insertln; end

    def keypad(p1) end
    alias keypad= keypad

    # def maxx
    def maxx; end

    # def maxy
    def maxy; end

    # def move(y, x)
    def move(p1, p2) end

    def nodelay=(p1) end

    # def noutrefresh
    def noutrefresh; end

    # def refresh
    def refresh; end

    def resize(p1, p2) end

    def scrl(p1) end

    # USE_COLOR
    def scroll; end

    def scrollok(p1) end

    # def setpos(y, x)
    def setpos(p1, p2) end

    def setscrreg(p1, p2) end

    # def standend
    def standend; end

    # def standout
    def standout; end

    # def subwin(height, width, top, left)
    def subwin(p1, p2, p3, p4) end

    def timeout=(p1) end
  end
end
