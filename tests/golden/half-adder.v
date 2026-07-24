module Half_Adder (
    a,
    b,
    carry,
    sum
);

input wire a;
input wire b;
output wire carry;
output wire sum;

wire a__2;
wire b__2;
wire carry__2;
wire sum__2;

assign a__2 = a;
assign b__2 = b;
assign carry = carry__2;
assign sum = sum__2;

assign carry__2 = a__2 & b__2;
assign sum__2 = a__2 ^ b__2;

endmodule
