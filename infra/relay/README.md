# WOMBAT P2P relay en AWS

Esta configuración crea una única instancia EC2 Ubuntu Server 24.04 LTS en
`us-east-1` (configurable mediante `aws_region`). La instancia usa `t3.micro`
por defecto, una raíz gp3 cifrada de 8 GiB y una IP pública. Su grupo de
seguridad permite conexiones P2P TCP por el puerto 9000 desde
`0.0.0.0/0` por defecto; no abre SSH.
El tráfico saliente se limita a HTTP/HTTPS para actualizaciones y SSM, y DNS
hacia la VPC para resolver los endpoints necesarios.

La administración se realiza con AWS Systems Manager Session Manager mediante
el rol `AmazonSSMManagedInstanceCore`. No se crean claves SSH ni key pairs.
Terraform gestiona la Default VPC de AWS, y busca una subnet disponible en esa
VPC. Si la cuenta no tenía Default VPC en la región, Terraform la crea. La AMI
oficial Canonical más reciente de Ubuntu 24.04 LTS x86_64 se obtiene mediante un
data source.

## Requisitos

- Terraform `>= 1.5.0`.
- Credenciales AWS configuradas localmente con permisos para gestionar la
  Default VPC, consultar subnets y AMIs, y planificar los recursos. La instancia
  requiere permisos para EC2, IAM, perfiles de instancia y grupos de seguridad.

## Uso

Desde este directorio:

```sh
terraform init
terraform fmt -check
terraform validate
terraform plan
```

`terraform init` descarga el provider AWS y genera `.terraform.lock.hcl`, que
debe conservarse en el control de versiones. `terraform fmt -check` comprueba
el formato sin modificar archivos. `terraform validate` valida la configuración
y `terraform plan` muestra los cambios previstos sin crear recursos.

Para ajustar valores, crea un `terraform.tfvars` local, por ejemplo:

```hcl
aws_region        = "us-east-1"
instance_type     = "t3.micro"
p2p_port          = 9000
p2p_allowed_cidrs = ["0.0.0.0/0"]
name_prefix       = "wombat-relay"
```

El archivo `terraform.tfvars` se ignora en Git. Para permitir acceso P2P solo
desde redes concretas, reemplaza `p2p_allowed_cidrs` por sus CIDR.

## Costos y eliminación

`terraform apply` crea la instancia y los recursos de soporte, lo que genera
costos de AWS mientras estén activos. No ejecutes `apply` sin revisar el plan y
aceptar esos costos. La instancia EC2, su volumen EBS gp3 y la IPv4 pública
asociada pueden generar costos. `terraform destroy` elimina la EC2 y los
recursos propios de este proyecto, pero no se debe asumir que elimina la
Default VPC de la cuenta: AWS la trata como una VPC predeterminada de la cuenta,
y Terraform deja de gestionarla al destruir este recurso.

## Conexión con Session Manager

En AWS Console, abre **EC2 > Instances**, selecciona el relay y elige
**Connect > Session Manager > Connect**. También puedes buscar el ID de la
instancia con el output `instance_id`. La instancia debe estar registrada como
nodo administrado en Systems Manager para que la conexión esté disponible.
La dirección P2P se muestra en el output `p2p_listen_address`.
