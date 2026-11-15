variable "aws_region" {
  description = "AWS region where the WOMBAT relay will be deployed."
  type        = string
  default     = "us-east-1"
}

variable "instance_type" {
  description = "EC2 instance type for the relay."
  type        = string
  default     = "t3.micro"
}

variable "p2p_port" {
  description = "TCP port used by the WOMBAT P2P relay."
  type        = number
  default     = 9000
}

variable "p2p_allowed_cidrs" {
  description = "IPv4 CIDR blocks allowed to connect to the P2P port."
  type        = list(string)
  default     = ["0.0.0.0/0"]
}

variable "name_prefix" {
  description = "Prefix used for names of WOMBAT relay resources."
  type        = string
  default     = "wombat-relay"
}
