module UserManagement
  module Authentication
    def authenticate(password)
      hash_password(password) == @password_hash
    end

    private

    def hash_password(password)
      # Complex password hashing logic
      Digest::SHA256.hexdigest(password + @salt)
    end
  end
end
