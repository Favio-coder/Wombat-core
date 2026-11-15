output "instance_id" {
  description = "EC2 instance ID of the WOMBAT P2P relay."
  value       = aws_instance.relay.id
}

output "public_ip" {
  description = "Public IPv4 address of the relay."
  value       = aws_instance.relay.public_ip
}

output "p2p_listen_address" {
  description = "Multiaddress peers can use to reach the relay over TCP."
  value       = "/ip4/${aws_instance.relay.public_ip}/tcp/${var.p2p_port}"
}

output "security_group_id" {
  description = "ID of the relay-exclusive security group."
  value       = aws_security_group.relay.id
}

output "session_manager_console_instructions" {
  description = "AWS Console path to open a Session Manager session to the relay."
  value       = "AWS Console > EC2 > Instances > select ${aws_instance.relay.id} > Connect > Session Manager > Connect."
}
