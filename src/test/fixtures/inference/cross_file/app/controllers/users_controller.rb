class UserService
  def list_users
    []
  end
end

class UsersController
  def initialize
    @service = UserService.new
  end

  def index
    users = @service.list_users
    @service.
    users
  end
end
