module Full_Adder (
    a,
    b,
    cin,
    cout,
    sum
);

input wire a;
input wire b;
input wire cin;
output wire cout;
output wire sum;

wire a__2;
wire ab_carry;
wire ab_xor;
wire b__2;
wire cin__2;
wire cin_carry;
wire cout__2;
wire sum__2;

assign a__2 = a;
assign b__2 = b;
assign cin__2 = cin;
assign cout = cout__2;
assign sum = sum__2;

assign ab_carry = a__2 & b__2;
assign cin_carry = ab_xor & cin__2;
assign cout__2 = ab_carry | cin_carry;
assign ab_xor = a__2 ^ b__2;
assign sum__2 = ab_xor ^ cin__2;

endmodule
